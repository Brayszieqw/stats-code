// ---------------------------------------------------------------------------
// Cox proportional hazards regression analysis module.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::ModelCoxArgs;
use crate::helpers::{join_or_placeholder, merge_unique_strings, require_column, stringify_error};
use crate::logistic::{
    build_nonintercept_terms, parse_binary_outcome, reference_note_for_plan,
    resolve_logistic_variable_plan,
};
use crate::math::{
    compute_cox_concordance, dot, invert_matrix_with_ridge, matrix_vector_mul, normal_cdf,
    safe_exp, CoxObservation,
};
use crate::modeling::{CoxFit, RowState};
use crate::schema::{is_missing_value, AnalysisSpec, CoxCoefficient, CoxResult};

type CoxPartialStats = (f64, Vec<f64>, Vec<Vec<f64>>);

pub(crate) fn cox_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    analysis_spec: Option<&AnalysisSpec>,
    args: &ModelCoxArgs,
) -> Result<CoxResult, String> {
    let predictors = merge_unique_strings(
        &args.predictors,
        &args.adjust,
        &[args.time.clone(), args.event.clone()],
    );
    if predictors.is_empty() {
        return Err("Cox requires at least one predictor or adjustment variable.".to_string());
    }

    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader
        .headers()
        .map_err(stringify_error)?
        .iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let header_index = headers
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let time_index = require_column(&header_index, &args.time)?;
    let event_index = require_column(&header_index, &args.event)?;
    let predictor_indices = predictors
        .iter()
        .map(|name| require_column(&header_index, name).map(|index| (name.clone(), index)))
        .collect::<Result<Vec<_>, _>>()?;

    let records = reader
        .records()
        .map(|record| record.map_err(stringify_error))
        .collect::<Result<Vec<_>, _>>()?;
    let n_total = records.len();
    if n_total == 0 {
        return Err("Dataset contains no data rows.".to_string());
    }

    let variable_plans = predictor_indices
        .iter()
        .map(|(name, index)| resolve_logistic_variable_plan(name, *index, analysis_spec, &records))
        .collect::<Vec<_>>();
    let design_terms = build_nonintercept_terms(&variable_plans);
    if design_terms.is_empty() {
        return Err(
            "No usable predictors remained after encoding and constant-column checks.".to_string(),
        );
    }

    let mut observations = Vec::new();
    let mut n_excluded_missing = 0usize;
    let mut n_excluded_invalid = 0usize;

    for record in &records {
        let time_raw = record.get(time_index).unwrap_or_default();
        let event_raw = record.get(event_index).unwrap_or_default();
        let time_trimmed = time_raw.trim();
        let event_trimmed = event_raw.trim();
        if is_missing_value(time_trimmed) || is_missing_value(event_trimmed) {
            n_excluded_missing += 1;
            continue;
        }
        let Ok(time_value) = time_trimmed.parse::<f64>() else {
            n_excluded_invalid += 1;
            continue;
        };
        let Some(event_value) = parse_binary_outcome(event_trimmed) else {
            n_excluded_invalid += 1;
            continue;
        };
        if !time_value.is_finite() || time_value <= 0.0 {
            n_excluded_invalid += 1;
            continue;
        }

        let mut row = Vec::new();
        let mut row_state = RowState::Ok;
        for plan in &variable_plans {
            let value = record.get(plan.source_index).unwrap_or_default();
            match plan.append_design_values(value, &mut row) {
                Ok(()) => {}
                Err(RowState::Ok) => {
                    unreachable!("append_design_values cannot return Err(RowState::Ok)")
                }
                Err(RowState::Missing) => {
                    row_state = RowState::Missing;
                    break;
                }
                Err(RowState::Invalid) => {
                    row_state = RowState::Invalid;
                    break;
                }
            }
        }

        match row_state {
            RowState::Ok => observations.push(CoxObservation {
                time: time_value,
                event: event_value >= 0.5,
                x: row,
            }),
            RowState::Missing => n_excluded_missing += 1,
            RowState::Invalid => n_excluded_invalid += 1,
        }
    }

    let n_used = observations.len();
    if n_used == 0 {
        return Err("No complete analyzable rows remained for Cox regression.".to_string());
    }

    let n_events = observations
        .iter()
        .filter(|observation| observation.event)
        .count();
    let n_censored = n_used.saturating_sub(n_events);
    if n_events == 0 {
        return Err("Cox regression requires at least one observed event.".to_string());
    }
    let tied_event_times = count_tied_event_times(&observations);

    let fit = fit_cox_newton(&observations)?;
    let coefficients = design_terms
        .iter()
        .enumerate()
        .map(|(index, term)| {
            let beta = fit.beta[index];
            let standard_error = fit.standard_errors[index];
            let z_value = if standard_error > 0.0 {
                beta / standard_error
            } else {
                0.0
            };
            let p_value = if standard_error > 0.0 {
                2.0 * (1.0 - normal_cdf(z_value.abs()))
            } else {
                1.0
            };
            let ci_lower_beta = beta - 1.959_963_984_540_054 * standard_error;
            let ci_upper_beta = beta + 1.959_963_984_540_054 * standard_error;
            CoxCoefficient {
                term: term.term.clone(),
                variable: term.variable.clone(),
                level: term.level.clone(),
                reference: term.reference.clone(),
                beta,
                standard_error,
                hazard_ratio: safe_exp(beta),
                ci_lower: safe_exp(ci_lower_beta),
                ci_upper: safe_exp(ci_upper_beta),
                p_value,
            }
        })
        .collect::<Vec<_>>();

    let parameter_count = coefficients.len().max(1);
    let epv = n_events as f64 / parameter_count as f64;
    let mut warnings = variable_plans
        .iter()
        .filter_map(super::modeling::LogisticVariablePlan::warning)
        .collect::<Vec<_>>();
    if !fit.converged {
        warnings.push("model_did_not_converge_within_max_iterations".to_string());
    }
    if epv < 10.0 {
        warnings.push(format!("low_events_per_parameter={epv:.2}"));
    }
    if tied_event_times > 0 {
        warnings.push(format!("tied_event_times_present={tied_event_times}"));
    }
    if coefficients
        .iter()
        .any(|coefficient| coefficient.beta.abs() >= 5.0)
    {
        warnings.push("possible_extreme_coefficients_or_instability".to_string());
    }
    warnings
        .push("verify_proportional_hazards_with_log_log_plot_or_schoenfeld_residuals".to_string());

    let mut notes = vec![
        "Complete-case Cox proportional hazards model with local deterministic fitting."
            .to_string(),
        "Ties are handled with the Breslow approximation.".to_string(),
        format!("Events per parameter: {epv:.2}."),
    ];
    notes.extend(variable_plans.iter().filter_map(reference_note_for_plan));
    if n_excluded_missing > 0 {
        notes.push(format!(
            "Excluded {n_excluded_missing} rows because time, event, or predictors were missing."
        ));
    }
    if n_excluded_invalid > 0 {
        notes.push(format!(
            "Excluded {n_excluded_invalid} rows because time, event, or numeric covariates were invalid."
        ));
    }

    let concordance = compute_cox_concordance(&observations, &fit.beta);

    Ok(CoxResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        formula: build_cox_formula(&args.time, &args.event, &args.predictors, &args.adjust),
        time: args.time.clone(),
        event: args.event.clone(),
        predictors,
        n_total,
        n_used,
        n_excluded_missing,
        n_excluded_invalid,
        n_events,
        n_censored,
        tied_event_times,
        iterations: fit.iterations,
        converged: fit.converged,
        log_partial_likelihood: fit.log_partial_likelihood,
        concordance: Some(concordance),
        coefficients,
        notes,
        warnings,
    })
}

pub(crate) fn fit_cox_newton(observations: &[CoxObservation]) -> Result<CoxFit, String> {
    let p = observations
        .first()
        .map_or(0, |observation| observation.x.len());
    if p == 0 {
        return Err("Empty Cox design matrix.".to_string());
    }

    let mut beta = vec![0.0; p];
    let mut converged = false;
    let mut iterations = 0usize;
    let max_iterations = 50usize;
    let tolerance = 1e-8_f64;

    for iteration in 0..max_iterations {
        let (_, gradient, information) = cox_partial_stats(observations, &beta)?;
        let information_inverse = invert_matrix_with_ridge(&information)?;
        let step = matrix_vector_mul(&information_inverse, &gradient);
        let max_step = step
            .iter()
            .fold(0.0_f64, |current, value| current.max(value.abs()));
        for index in 0..p {
            beta[index] += step[index];
        }
        iterations = iteration + 1;
        if max_step < tolerance {
            converged = true;
            break;
        }
    }

    let (log_partial_likelihood, _, information) = cox_partial_stats(observations, &beta)?;
    let covariance = invert_matrix_with_ridge(&information)?;
    let standard_errors = (0..p)
        .map(|index| covariance[index][index].max(0.0).sqrt())
        .collect::<Vec<_>>();

    Ok(CoxFit {
        beta,
        standard_errors,
        iterations,
        converged,
        log_partial_likelihood,
    })
}

pub(crate) fn build_cox_formula(
    time: &str,
    event: &str,
    predictors: &[String],
    adjust: &[String],
) -> String {
    let terms = predictors
        .iter()
        .chain(adjust.iter())
        .cloned()
        .collect::<Vec<_>>();
    format!(
        "coxph(Surv({time}, {event}) ~ {})",
        join_or_placeholder(&terms, "predictors required")
    )
}

pub(crate) fn count_tied_event_times(observations: &[CoxObservation]) -> usize {
    let mut event_times = observations
        .iter()
        .filter(|observation| observation.event)
        .map(|observation| observation.time)
        .collect::<Vec<_>>();
    event_times.sort_by(f64::total_cmp);
    let mut tied = 0usize;
    let mut index = 0usize;
    while index < event_times.len() {
        let time = event_times[index];
        let mut group_size = 1usize;
        index += 1;
        while index < event_times.len() && event_times[index] == time {
            group_size += 1;
            index += 1;
        }
        if group_size > 1 {
            tied += 1;
        }
    }
    tied
}

#[allow(clippy::needless_range_loop)]
pub(crate) fn cox_partial_stats(
    observations: &[CoxObservation],
    beta: &[f64],
) -> Result<CoxPartialStats, String> {
    let p = beta.len();
    let mut event_entries = observations
        .iter()
        .enumerate()
        .filter(|(_, observation)| observation.event)
        .map(|(index, observation)| (index, observation.time))
        .collect::<Vec<_>>();
    event_entries.sort_by(|left, right| left.1.total_cmp(&right.1));

    let mut log_partial_likelihood = 0.0_f64;
    let mut gradient = vec![0.0; p];
    let mut information = vec![vec![0.0; p]; p];
    let linear_predictors = observations
        .iter()
        .map(|observation| dot(&observation.x, beta))
        .collect::<Vec<_>>();

    let mut event_position = 0usize;
    while event_position < event_entries.len() {
        let time = event_entries[event_position].1;
        let mut event_indices = Vec::new();
        while event_position < event_entries.len() && event_entries[event_position].1 == time {
            event_indices.push(event_entries[event_position].0);
            event_position += 1;
        }

        let d = event_indices.len() as f64;
        let mut s0 = 0.0_f64;
        let mut s1 = vec![0.0; p];
        let mut s2 = vec![vec![0.0; p]; p];
        let risk_set_indices = observations
            .iter()
            .enumerate()
            .filter(|(_, observation)| observation.time >= time)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(max_eta) = risk_set_indices
            .iter()
            .map(|index| linear_predictors[*index])
            .max_by(f64::total_cmp)
        else {
            return Err("Invalid Cox risk set encountered during fitting.".to_string());
        };
        for index in risk_set_indices {
            let observation = &observations[index];
            let risk_score = (linear_predictors[index] - max_eta).exp();
            s0 += risk_score;
            for j in 0..p {
                s1[j] += risk_score * observation.x[j];
                for k in 0..p {
                    s2[j][k] += risk_score * observation.x[j] * observation.x[k];
                }
            }
        }
        if s0 <= 0.0 || !s0.is_finite() {
            return Err("Invalid Cox risk set encountered during fitting.".to_string());
        }

        let mut event_sum = vec![0.0; p];
        for index in &event_indices {
            log_partial_likelihood += linear_predictors[*index];
            for j in 0..p {
                event_sum[j] += observations[*index].x[j];
            }
        }
        log_partial_likelihood -= d * (max_eta + s0.ln());
        for j in 0..p {
            gradient[j] += event_sum[j] - d * s1[j] / s0;
            for k in 0..p {
                information[j][k] += d * (s2[j][k] / s0 - (s1[j] * s1[k]) / (s0 * s0));
            }
        }
    }

    Ok((log_partial_likelihood, gradient, information))
}
