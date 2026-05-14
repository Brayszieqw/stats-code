// ---------------------------------------------------------------------------
// Cox proportional hazards regression analysis module.
// ---------------------------------------------------------------------------

//! CSV-backed Cox proportional hazards workflow.
//!
//! This module validates time-to-event inputs, reuses the shared predictor
//! encoding path for categorical and continuous terms, fits a Cox proportional
//! hazards model, and emits diagnostics that explain exclusions, convergence,
//! and unstable estimates for downstream reports.
//!
//! The fitter maximizes the log partial likelihood with Newton iterations over
//! observed event times. Risk sets use `exp(X * beta)` with a max-eta shift for
//! numerical stability; tied failures at the same time are handled with the
//! Efron approximation (the default used by `lifelines` and R `survival`'s
//! `ties="efron"`), which subtracts `Σ_{k=0..d−1} log(S0 − (k/d) · S0_events)`
//! per tied group instead of the coarser Breslow `d · log(S0)`. Coefficients
//! are reported as hazard ratios with Wald intervals from the inverted
//! information matrix.

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::ModelCoxArgs;
use crate::helpers::{
    join_or_placeholder, merge_unique_strings, parse_positive_weight, require_column,
    stringify_error,
};
use crate::logistic::{
    build_nonintercept_terms, parse_binary_outcome, reference_note_for_plan,
    resolve_logistic_variable_plan,
};
use crate::math::{
    chi_square_cdf, compute_cox_concordance, dot, invert_matrix_with_ridge, matrix_vector_mul,
    normal_cdf, safe_exp, CoxObservation,
};
use crate::modeling::{CoxFit, RowState};
use crate::schema::{
    is_missing_value_for_column, AnalysisSpec, CoxCoefficient, CoxPhDiagnostic, CoxResult,
};

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
    let survey_weight = analysis_spec
        .and_then(|spec| spec.survey.as_ref())
        .and_then(|survey| survey.weight.clone());
    let weight_index = survey_weight
        .as_ref()
        .map(|name| require_column(&header_index, name).map(|index| (name.clone(), index)))
        .transpose()?;
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
    let mut n_excluded_missing_weight = 0usize;
    let mut n_excluded_invalid_weight = 0usize;

    for record in &records {
        let time_raw = record.get(time_index).unwrap_or_default();
        let event_raw = record.get(event_index).unwrap_or_default();
        let time_trimmed = time_raw.trim();
        let event_trimmed = event_raw.trim();
        if is_missing_value_for_column(&args.time, time_trimmed)
            || is_missing_value_for_column(&args.event, event_trimmed)
        {
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
            RowState::Ok => {
                let weight = if let Some((weight_name, weight_index)) = &weight_index {
                    match parse_positive_weight(
                        weight_name,
                        record.get(*weight_index).unwrap_or_default(),
                    ) {
                        Ok(Some(value)) => value,
                        Ok(None) => {
                            n_excluded_missing_weight += 1;
                            continue;
                        }
                        Err(_) => {
                            n_excluded_invalid_weight += 1;
                            continue;
                        }
                    }
                } else {
                    1.0
                };
                observations.push(CoxObservation {
                    time: time_value,
                    event: event_value >= 0.5,
                    x: row,
                    weight,
                });
            }
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
        "Ties are handled with the Efron approximation (matches lifelines default).".to_string(),
        format!("Events per parameter: {epv:.2}."),
    ];
    if let Some(weight) = &survey_weight {
        notes.push(format!(
            "Survey weight `{weight}` was applied as an observation weight in the partial likelihood."
        ));
        notes.push(
            "Complex survey design variance, strata, clusters, and replicate weights are not applied to model standard errors."
                .to_string(),
        );
        notes.push(format!(
            "Excluded {n_excluded_missing_weight} rows with missing `{weight}` and {n_excluded_invalid_weight} rows with invalid/non-positive `{weight}`."
        ));
    }
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
    let ph_diagnostics = cox_ph_diagnostics(&observations, &fit.beta, &coefficients);

    Ok(CoxResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        formula: build_cox_formula(&args.time, &args.event, &args.predictors, &args.adjust),
        time: args.time.clone(),
        event: args.event.clone(),
        predictors,
        survey_weight,
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
        ph_diagnostics,
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

pub(crate) fn cox_ph_diagnostics(
    observations: &[CoxObservation],
    beta: &[f64],
    coefficients: &[CoxCoefficient],
) -> Vec<CoxPhDiagnostic> {
    let p = beta.len();
    if p == 0 || coefficients.len() != p {
        return Vec::new();
    }
    let linear_predictors = observations
        .iter()
        .map(|observation| dot(&observation.x, beta))
        .collect::<Vec<_>>();
    let mut event_entries = observations
        .iter()
        .enumerate()
        .filter(|(_, observation)| observation.event && observation.time > 0.0)
        .map(|(index, observation)| (index, observation.time))
        .collect::<Vec<_>>();
    event_entries.sort_by(|left, right| left.1.total_cmp(&right.1));

    let mut log_times_by_term = vec![Vec::<f64>::new(); p];
    let mut residuals_by_term = vec![Vec::<f64>::new(); p];

    for (event_index, event_time) in event_entries {
        let risk_set_indices = observations
            .iter()
            .enumerate()
            .filter(|(_, observation)| observation.time >= event_time)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let Some(max_eta) = risk_set_indices
            .iter()
            .map(|index| linear_predictors[*index])
            .max_by(f64::total_cmp)
        else {
            continue;
        };
        let mut s0 = 0.0_f64;
        let mut s1 = vec![0.0; p];
        for index in risk_set_indices {
            let observation = &observations[index];
            let risk_score = observation.weight * (linear_predictors[index] - max_eta).exp();
            s0 += risk_score;
            for (term_index, value) in observation.x.iter().enumerate().take(p) {
                s1[term_index] += risk_score * value;
            }
        }
        if s0 <= 0.0 || !s0.is_finite() {
            continue;
        }
        let log_time = event_time.ln();
        for term_index in 0..p {
            let expected = s1[term_index] / s0;
            let residual = observations[event_index].x[term_index] - expected;
            if residual.is_finite() && log_time.is_finite() {
                log_times_by_term[term_index].push(log_time);
                residuals_by_term[term_index].push(residual);
            }
        }
    }

    coefficients
        .iter()
        .enumerate()
        .map(|(term_index, coefficient)| {
            let correlation = pearson_correlation(
                &log_times_by_term[term_index],
                &residuals_by_term[term_index],
            )
            .unwrap_or(0.0);
            let event_count = residuals_by_term[term_index].len();
            let chi_square = event_count as f64 * correlation * correlation;
            let p_value = if event_count >= 3 {
                (1.0 - chi_square_cdf(chi_square, 1.0)).clamp(0.0, 1.0)
            } else {
                f64::NAN
            };
            CoxPhDiagnostic {
                term: coefficient.term.clone(),
                correlation,
                chi_square,
                p_value,
                event_count,
            }
        })
        .collect()
}

fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() != y.len() || x.len() < 3 {
        return None;
    }
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let mut sum_sq_x = 0.0;
    let mut sum_sq_y = 0.0;
    let mut sum_cross_product = 0.0;
    for (x_value, y_value) in x.iter().zip(y.iter()) {
        let dx = x_value - mean_x;
        let dy = y_value - mean_y;
        sum_sq_x += dx * dx;
        sum_sq_y += dy * dy;
        sum_cross_product += dx * dy;
    }
    let denominator = (sum_sq_x * sum_sq_y).sqrt();
    if denominator > 0.0 && denominator.is_finite() {
        Some(sum_cross_product / denominator)
    } else {
        None
    }
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

        let d = event_indices
            .iter()
            .map(|index| observations[*index].weight)
            .sum::<f64>();
        let d_count = event_indices.len() as f64;
        let mut s0 = 0.0_f64;
        let mut s1 = vec![0.0; p];
        let mut s2 = vec![vec![0.0; p]; p];
        // Efron tie handling needs the analogous sums restricted to the event
        // subset D(t). They are accumulated on the same max_eta scale as S0.
        let mut se0 = 0.0_f64;
        let mut se1 = vec![0.0; p];
        let mut se2 = vec![vec![0.0; p]; p];
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
            let risk_score = observation.weight * (linear_predictors[index] - max_eta).exp();
            s0 += risk_score;
            for j in 0..p {
                s1[j] += risk_score * observation.x[j];
                for k in 0..p {
                    s2[j][k] += risk_score * observation.x[j] * observation.x[k];
                }
            }
        }
        for index in &event_indices {
            let observation = &observations[*index];
            let risk_score = observation.weight * (linear_predictors[*index] - max_eta).exp();
            se0 += risk_score;
            for j in 0..p {
                se1[j] += risk_score * observation.x[j];
                for k in 0..p {
                    se2[j][k] += risk_score * observation.x[j] * observation.x[k];
                }
            }
        }
        if s0 <= 0.0 || !s0.is_finite() {
            return Err("Invalid Cox risk set encountered during fitting.".to_string());
        }

        let mut event_sum = vec![0.0; p];
        for index in &event_indices {
            let event_weight = observations[*index].weight;
            log_partial_likelihood += event_weight * linear_predictors[*index];
            for j in 0..p {
                event_sum[j] += event_weight * observations[*index].x[j];
            }
        }

        // Efron correction: iterate over the d tied events in the group.
        // Reduces to Breslow when d_count == 1.
        let d_steps = event_indices.len().max(1);
        for step in 0..d_steps {
            let fraction = step as f64 / d_count;
            let denom = s0 - fraction * se0;
            if denom <= 0.0 || !denom.is_finite() {
                return Err("Invalid Cox risk set encountered during fitting.".to_string());
            }
            // log(exp(max_eta) * denom) = max_eta + ln(denom)
            log_partial_likelihood -= (d / d_count) * (max_eta + denom.ln());
            for j in 0..p {
                let num_j = s1[j] - fraction * se1[j];
                gradient[j] -= (d / d_count) * num_j / denom;
                for k in 0..p {
                    let num_jk = s2[j][k] - fraction * se2[j][k];
                    information[j][k] += (d / d_count)
                        * (num_jk / denom - (num_j * (s1[k] - fraction * se1[k])) / (denom * denom));
                }
            }
        }
        for j in 0..p {
            gradient[j] += event_sum[j];
        }
    }

    Ok((log_partial_likelihood, gradient, information))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{compute_cox_concordance, CoxObservation};

    /// Test partial likelihood for a single-event dataset.
    ///
    /// With one event at time=5 (x=[1.0]) and one censored at time=10 (x=[0.0]),
    /// beta=[0.0]: risk set at time=5 includes both observations.
    /// `log_partial_likelihood` = `x_event` * beta - d * `ln(sum_risk)`
    ///   = 1.0*0.0 - 1 * ln(exp(0) + exp(0)) = -ln(2)
    #[test]
    fn partial_likelihood_single_event() {
        let observations = vec![
            CoxObservation {
                time: 5.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 10.0,
                event: false,
                x: vec![0.0],
                weight: 1.0,
            },
        ];
        let beta = vec![0.0];
        let (log_pl, gradient, information) = cox_partial_stats(&observations, &beta).unwrap();

        // At beta=0: log PL = 0 - ln(exp(0)+exp(0)) = -ln(2)
        let expected_log_pl = -(2.0_f64.ln());
        assert!(
            (log_pl - expected_log_pl).abs() < 1e-12,
            "log partial likelihood: expected {expected_log_pl}, got {log_pl}"
        );

        // Gradient: x_event - d * s1/s0 = 1.0 - 1*(1+0)/(1+1) = 1.0 - 0.5 = 0.5
        assert!(
            (gradient[0] - 0.5).abs() < 1e-12,
            "gradient[0]: expected 0.5, got {}",
            gradient[0]
        );

        // Information: d * (s2/s0 - (s1/s0)^2) = 1 * (1/2 - (1/2)^2) = 0.25
        assert!(
            (information[0][0] - 0.25).abs() < 1e-12,
            "information[0][0]: expected 0.25, got {}",
            information[0][0]
        );
    }

    /// Test that `count_tied_event_times` correctly identifies tied event times.
    #[test]
    fn tied_events_counting() {
        // Two events at time=3, one event at time=5, two events at time=7
        // → 2 groups with ties (time=3 and time=7)
        let observations = vec![
            CoxObservation {
                time: 3.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 3.0,
                event: true,
                x: vec![0.5],
                weight: 1.0,
            },
            CoxObservation {
                time: 5.0,
                event: true,
                x: vec![0.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 7.0,
                event: true,
                x: vec![1.5],
                weight: 1.0,
            },
            CoxObservation {
                time: 7.0,
                event: true,
                x: vec![2.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 8.0,
                event: false,
                x: vec![0.3],
                weight: 1.0,
            },
        ];
        let tied = count_tied_event_times(&observations);
        assert_eq!(tied, 2, "expected 2 tied event time groups, got {tied}");

        // No ties: all distinct event times
        let no_ties = vec![
            CoxObservation {
                time: 1.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 2.0,
                event: true,
                x: vec![0.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 3.0,
                event: false,
                x: vec![0.5],
                weight: 1.0,
            },
        ];
        assert_eq!(count_tied_event_times(&no_ties), 0);
    }

    /// Test that the Efron correction reduces to the exact Breslow result when
    /// every tied group has size 1 (i.e. no ties).
    #[test]
    fn partial_likelihood_no_ties_matches_single_event() {
        // Same dataset as `partial_likelihood_single_event`: with d=1 the Efron
        // iteration collapses to a single step at fraction=0, identical to the
        // previous Breslow term `d · log(S0)`.
        let observations = vec![
            CoxObservation {
                time: 5.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 10.0,
                event: false,
                x: vec![0.0],
                weight: 1.0,
            },
        ];
        let (log_pl, _, _) = cox_partial_stats(&observations, &[0.0]).unwrap();
        let expected = -(2.0_f64.ln());
        assert!(
            (log_pl - expected).abs() < 1e-12,
            "Efron with d=1 must equal Breslow: expected {expected}, got {log_pl}"
        );
    }

    /// Verify the Efron tie correction against a hand-computed value and show
    /// it differs from the Breslow approximation when ties are present.
    ///
    /// Setup at beta = 0:
    ///   * Two tied events at time = 1 with x = 1 and x = 0.
    ///   * One censored observation at time = 2 with x = 0.
    ///   * Risk set at t=1 has S0 = 3, event subset has SE0 = 2.
    ///   * Event score sum = 1·0 + 0·0 = 0.
    ///
    /// ```text
    /// Efron log PL   = 0 - [log(3) + log(3 - (1/2)·2)] = -log(3) - log(2)
    /// Breslow log PL = 0 - 2·log(3)
    /// ```
    #[test]
    fn partial_likelihood_efron_tied_group() {
        let observations = vec![
            CoxObservation {
                time: 1.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 1.0,
                event: true,
                x: vec![0.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 2.0,
                event: false,
                x: vec![0.0],
                weight: 1.0,
            },
        ];
        let (log_pl, _, _) = cox_partial_stats(&observations, &[0.0]).unwrap();
        let expected_efron = -(3.0_f64.ln()) - (2.0_f64.ln());
        let breslow = -2.0 * 3.0_f64.ln();
        assert!(
            (log_pl - expected_efron).abs() < 1e-12,
            "Efron log PL: expected {expected_efron}, got {log_pl}"
        );
        // And it must NOT equal the Breslow value — otherwise the correction
        // is silently disabled.
        assert!(
            (log_pl - breslow).abs() > 1e-6,
            "Efron must differ from Breslow when ties exist (breslow={breslow}, got={log_pl})"
        );
    }

    /// Test concordance edge case: when all linear predictors are equal,
    /// concordance should be 0.5 (random).
    #[test]
    fn concordance_all_tied_predictors() {
        // All observations have the same covariate value → same linear predictor
        // → all pairs are tied → concordance = 0.5
        let observations = vec![
            CoxObservation {
                time: 2.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 4.0,
                event: false,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 6.0,
                event: false,
                x: vec![1.0],
                weight: 1.0,
            },
        ];
        let beta = vec![1.0]; // doesn't matter, all x are the same
        let c = compute_cox_concordance(&observations, &beta);
        assert!(
            (c - 0.5).abs() < 1e-12,
            "concordance with tied predictors: expected 0.5, got {c}"
        );

        // Edge case: no usable pairs (single observation with event, no one survives longer)
        let single_event = vec![
            CoxObservation {
                time: 10.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 5.0,
                event: false,
                x: vec![0.0],
                weight: 1.0,
            },
        ];
        let c2 = compute_cox_concordance(&single_event, &[1.0]);
        // The event is at time=10, no j has time > 10, so total pairs = 0 → returns 0.5
        assert!(
            (c2 - 0.5).abs() < 1e-12,
            "concordance with no usable pairs: expected 0.5, got {c2}"
        );
    }
}
