// ---------------------------------------------------------------------------
// Logistic regression analysis module.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::ModelLogisticArgs;
use crate::helpers::{
    join_or_placeholder, merge_unique_strings, parse_event_value, require_column, stringify_error,
};
use crate::math::{
    compute_logistic_c_statistic, compute_nagelkerke_r2, compute_null_log_likelihood, dot,
    fisher_information, invert_matrix_with_ridge, matrix_vector_mul, normal_cdf, safe_exp, sigmoid,
};
use crate::modeling::{
    LogisticEncoding, LogisticFit, LogisticTermSpec, LogisticVariablePlan, RowState,
};
use crate::schema::{
    infer_variable_kind, is_missing_value, AnalysisSpec, LogisticCoefficient, LogisticResult,
    VariableKind,
};

pub(crate) fn logistic_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    analysis_spec: Option<&AnalysisSpec>,
    args: &ModelLogisticArgs,
) -> Result<LogisticResult, String> {
    let predictors = merge_unique_strings(
        &args.predictors,
        &args.adjust,
        std::slice::from_ref(&args.outcome),
    );
    if predictors.is_empty() {
        return Err("Logistic requires at least one predictor or adjustment variable.".to_string());
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
    let outcome_index = require_column(&header_index, &args.outcome)?;
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
    let design_terms = build_logistic_terms(&variable_plans);
    if design_terms.len() <= 1 {
        return Err(
            "No usable predictors remained after encoding and constant-column checks.".to_string(),
        );
    }

    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut n_excluded_missing = 0usize;
    let mut n_excluded_invalid = 0usize;

    for record in &records {
        let outcome_raw = record.get(outcome_index).unwrap_or_default();
        let Some(outcome) = parse_binary_outcome(outcome_raw) else {
            if is_missing_value(outcome_raw.trim()) {
                n_excluded_missing += 1;
            } else {
                n_excluded_invalid += 1;
            }
            continue;
        };

        let mut row = vec![1.0];
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
                x.push(row);
                y.push(outcome);
            }
            RowState::Missing => n_excluded_missing += 1,
            RowState::Invalid => n_excluded_invalid += 1,
        }
    }

    let n_used = y.len();
    if n_used == 0 {
        return Err("No complete analyzable rows remained for logistic regression.".to_string());
    }

    let n_events = y.iter().filter(|value| **value >= 0.5).count();
    let n_nonevents = n_used.saturating_sub(n_events);
    if n_events == 0 || n_nonevents == 0 {
        return Err(
            "Logistic regression requires both event and non-event observations.".to_string(),
        );
    }

    let fit = fit_logistic_newton(&x, &y)?;
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
            LogisticCoefficient {
                term: term.term.clone(),
                variable: term.variable.clone(),
                level: term.level.clone(),
                reference: term.reference.clone(),
                beta,
                standard_error,
                odds_ratio: safe_exp(beta),
                ci_lower: safe_exp(ci_lower_beta),
                ci_upper: safe_exp(ci_upper_beta),
                p_value,
            }
        })
        .collect::<Vec<_>>();

    let parameter_count = coefficients.len().saturating_sub(1).max(1);
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
    if n_used <= coefficients.len() {
        warnings.push("design_matrix_may_be_overparameterized".to_string());
    }
    if fit
        .fitted_probabilities
        .iter()
        .any(|probability| *probability <= 0.01 || *probability >= 0.99)
        || coefficients
            .iter()
            .any(|coefficient| coefficient.beta.abs() >= 8.0)
    {
        warnings.push("possible_separation_or_extreme_fitted_probabilities".to_string());
    }

    let notes = {
        let mut notes = vec![
            "Complete-case logistic regression with local deterministic fitting.".to_string(),
            "Categorical predictors are one-hot encoded with the declared or first observed level as reference.".to_string(),
            format!("Events per parameter: {epv:.2}."),
        ];
        notes.extend(variable_plans.iter().filter_map(reference_note_for_plan));
        if n_excluded_missing > 0 {
            notes.push(format!(
                "Excluded {n_excluded_missing} rows because outcome or predictors were missing."
            ));
        }
        if n_excluded_invalid > 0 {
            notes.push(format!(
                "Excluded {n_excluded_invalid} rows because binary outcome or numeric covariates were invalid."
            ));
        }
        notes
    };

    // Compute model diagnostics
    let null_log_likelihood = compute_null_log_likelihood(n_events, n_used);
    let pseudo_r2_nagelkerke =
        compute_nagelkerke_r2(fit.log_likelihood, null_log_likelihood, n_used);
    let k = coefficients.len() as f64;
    let n_f = n_used as f64;
    let aic = -2.0 * fit.log_likelihood + 2.0 * k;
    let bic = -2.0 * fit.log_likelihood + k * n_f.ln();
    let c_statistic = compute_logistic_c_statistic(&y, &fit.fitted_probabilities);

    Ok(LogisticResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        formula: build_logistic_formula(&args.outcome, &args.predictors, &args.adjust),
        outcome: args.outcome.clone(),
        predictors,
        n_total,
        n_used,
        n_excluded_missing,
        n_excluded_invalid,
        n_events,
        n_nonevents,
        iterations: fit.iterations,
        converged: fit.converged,
        log_likelihood: fit.log_likelihood,
        null_log_likelihood: Some(null_log_likelihood),
        pseudo_r2_nagelkerke: Some(pseudo_r2_nagelkerke),
        aic: Some(aic),
        bic: Some(bic),
        c_statistic: Some(c_statistic),
        coefficients,
        notes,
        warnings,
    })
}

pub(crate) fn build_logistic_formula(
    outcome: &str,
    predictors: &[String],
    adjust: &[String],
) -> String {
    let terms = predictors
        .iter()
        .chain(adjust.iter())
        .cloned()
        .collect::<Vec<_>>();
    format!(
        "logit({outcome} ~ {})",
        join_or_placeholder(&terms, "predictors required")
    )
}

pub(crate) fn resolve_logistic_variable_plan(
    name: &str,
    source_index: usize,
    analysis_spec: Option<&AnalysisSpec>,
    records: &[csv::StringRecord],
) -> LogisticVariablePlan {
    let variable_spec =
        analysis_spec.and_then(|spec| spec.variables.iter().find(|variable| variable.name == name));
    let kind = if let Some(variable) = variable_spec {
        variable.kind
    } else {
        let mut distinct_values = std::collections::BTreeSet::new();
        let mut non_missing_count = 0usize;
        let mut numeric_non_missing_count = 0usize;
        for record in records {
            let value = record.get(source_index).unwrap_or_default();
            let trimmed = value.trim();
            if is_missing_value(trimmed) {
                continue;
            }
            non_missing_count += 1;
            if trimmed.parse::<f64>().is_ok() {
                numeric_non_missing_count += 1;
            }
            if distinct_values.len() < 128 {
                distinct_values.insert(trimmed.to_string());
            }
        }
        infer_variable_kind(
            name,
            non_missing_count,
            numeric_non_missing_count,
            &distinct_values,
        )
    };

    if matches!(
        kind,
        VariableKind::Continuous | VariableKind::Time | VariableKind::PersonTime
    ) {
        let distinct_numeric = records
            .iter()
            .filter_map(|record| record.get(source_index))
            .map(str::trim)
            .filter(|value| !is_missing_value(value))
            .filter_map(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map(|value| format!("{value:.12}"))
            .collect::<std::collections::BTreeSet<_>>();
        let encoding = if distinct_numeric.len() <= 1 {
            LogisticEncoding::Omitted {
                reason: "constant_numeric_predictor".to_string(),
            }
        } else {
            LogisticEncoding::Continuous
        };
        return LogisticVariablePlan {
            name: name.to_string(),
            source_index,
            encoding,
        };
    }

    let observed_levels = records
        .iter()
        .filter_map(|record| record.get(source_index))
        .map(str::trim)
        .filter(|value| !is_missing_value(value))
        .map(ToOwned::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let mut ordered_levels = if let Some(variable) = variable_spec {
        if let Some(coding) = &variable.coding {
            if coding.levels.is_empty() {
                observed_levels.iter().cloned().collect::<Vec<_>>()
            } else {
                let mut levels = coding
                    .levels
                    .iter()
                    .filter(|level| observed_levels.contains(*level))
                    .cloned()
                    .collect::<Vec<_>>();
                let extras = observed_levels
                    .iter()
                    .filter(|level| !levels.contains(*level))
                    .cloned()
                    .collect::<Vec<_>>();
                levels.extend(extras);
                levels
            }
        } else {
            observed_levels.iter().cloned().collect::<Vec<_>>()
        }
    } else {
        observed_levels.iter().cloned().collect::<Vec<_>>()
    };
    if ordered_levels.len() <= 1 {
        return LogisticVariablePlan {
            name: name.to_string(),
            source_index,
            encoding: LogisticEncoding::Omitted {
                reason: "single_level_categorical_predictor".to_string(),
            },
        };
    }

    let reference = variable_spec
        .and_then(|variable| variable.coding.as_ref())
        .and_then(|coding| coding.reference.clone())
        .filter(|reference| ordered_levels.contains(reference))
        .unwrap_or_else(|| ordered_levels[0].clone());
    ordered_levels.retain(|level| level != &reference);

    LogisticVariablePlan {
        name: name.to_string(),
        source_index,
        encoding: LogisticEncoding::Dummy {
            reference,
            levels: ordered_levels,
        },
    }
}

pub(crate) fn build_logistic_terms(plans: &[LogisticVariablePlan]) -> Vec<LogisticTermSpec> {
    let mut terms = build_nonintercept_terms(plans);
    terms.insert(
        0,
        LogisticTermSpec {
            term: "Intercept".to_string(),
            variable: "Intercept".to_string(),
            level: None,
            reference: None,
        },
    );
    terms
}

pub(crate) fn build_nonintercept_terms(plans: &[LogisticVariablePlan]) -> Vec<LogisticTermSpec> {
    let mut terms = Vec::new();
    for plan in plans {
        match &plan.encoding {
            LogisticEncoding::Continuous => {
                terms.push(LogisticTermSpec {
                    term: plan.name.clone(),
                    variable: plan.name.clone(),
                    level: None,
                    reference: None,
                });
            }
            LogisticEncoding::Dummy { reference, levels } => {
                terms.extend(levels.iter().map(|level| LogisticTermSpec {
                    term: format!("{}[{level}]", plan.name),
                    variable: plan.name.clone(),
                    level: Some(level.clone()),
                    reference: Some(reference.clone()),
                }));
            }
            LogisticEncoding::Omitted { .. } => {}
        }
    }
    terms
}

pub(crate) fn parse_binary_outcome(raw: &str) -> Option<f64> {
    match parse_event_value(raw)? {
        value if (value - 0.0).abs() < f64::EPSILON => Some(0.0),
        value if (value - 1.0).abs() < f64::EPSILON => Some(1.0),
        _ => None,
    }
}

pub(crate) fn fit_logistic_newton(x: &[Vec<f64>], y: &[f64]) -> Result<LogisticFit, String> {
    let n = x.len();
    let p = x.first().map_or(0, std::vec::Vec::len);
    if n == 0 || p == 0 {
        return Err("Empty design matrix.".to_string());
    }

    let mut beta = vec![0.0; p];
    let mut converged = false;
    let mut iterations = 0usize;
    let max_iterations = 50usize;
    let tolerance = 1e-8_f64;

    for iteration in 0..max_iterations {
        let probabilities = x
            .iter()
            .map(|row| sigmoid(dot(row, &beta)).clamp(1e-9, 1.0 - 1e-9))
            .collect::<Vec<_>>();
        let mut fisher = vec![vec![0.0; p]; p];
        let mut gradient = vec![0.0; p];

        for (row_index, row) in x.iter().enumerate() {
            let probability = probabilities[row_index];
            let weight = (probability * (1.0 - probability)).max(1e-9);
            let residual = y[row_index] - probability;
            for j in 0..p {
                gradient[j] += row[j] * residual;
                for k in 0..p {
                    fisher[j][k] += row[j] * weight * row[k];
                }
            }
        }

        let fisher_inverse = invert_matrix_with_ridge(&fisher)?;
        let step = matrix_vector_mul(&fisher_inverse, &gradient);
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

    let fitted_probabilities = x
        .iter()
        .map(|row| sigmoid(dot(row, &beta)).clamp(1e-9, 1.0 - 1e-9))
        .collect::<Vec<_>>();
    let fisher = fisher_information(x, &fitted_probabilities);
    let covariance = invert_matrix_with_ridge(&fisher)?;
    let standard_errors = (0..p)
        .map(|index| covariance[index][index].max(0.0).sqrt())
        .collect::<Vec<_>>();
    let log_likelihood = y
        .iter()
        .zip(fitted_probabilities.iter())
        .map(|(observed, probability)| {
            observed * probability.ln() + (1.0 - observed) * (1.0 - probability).ln()
        })
        .sum();

    Ok(LogisticFit {
        beta,
        standard_errors,
        iterations,
        converged,
        log_likelihood,
        fitted_probabilities,
    })
}

pub(crate) fn reference_note_for_plan(plan: &LogisticVariablePlan) -> Option<String> {
    match &plan.encoding {
        LogisticEncoding::Dummy { reference, .. } => {
            Some(format!("{} reference level: {reference}.", plan.name))
        }
        _ => None,
    }
}
