// ---------------------------------------------------------------------------
// Linear regression (OLS) analysis module.
// ---------------------------------------------------------------------------

//! CSV-backed ordinary least squares workflow.
//!
//! This module builds the same encoded predictor matrix used by other model
//! commands, applies study-context missing-value rules and optional weights,
//! solves weighted OLS, and returns report-ready estimates, intervals, p-values,
//! and model diagnostics.
//!
//! Estimates are computed from the normal equations `(X'WX)^-1 X'Wy`. Standard
//! errors use the residual mean square and diagonal of `(X'WX)^-1`; term tests
//! use t statistics, and the model-level statistic uses the usual regression
//! mean-square ratio. This keeps the implementation deterministic and suitable
//! for small to medium CSV analyses without an external statistics runtime.

use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::ModelLinearArgs;
use crate::helpers::{
    merge_unique_strings, parse_positive_weight, require_column, stringify_error,
};
use crate::logistic::{build_logistic_terms, resolve_logistic_variable_plan};
use crate::math::{
    f_distribution_p_value, invert_matrix, t_critical_value_95, t_distribution_p_value,
};
use crate::modeling::{LogisticTermSpec, RowState};
use crate::schema::{is_missing_value_for_column, AnalysisSpec, LinearCoefficient, LinearResult};

pub(crate) fn linear_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    analysis_spec: Option<&AnalysisSpec>,
    args: &ModelLinearArgs,
) -> Result<LinearResult, String> {
    let predictors = merge_unique_strings(
        &args.predictors,
        &args.adjust,
        std::slice::from_ref(&args.outcome),
    );
    if predictors.is_empty() {
        return Err(
            "Linear regression requires at least one predictor or adjustment variable.".to_string(),
        );
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
    let design_terms = build_logistic_terms(&variable_plans);
    if design_terms.len() <= 1 {
        return Err(
            "No usable predictors remained after encoding and constant-column checks.".to_string(),
        );
    }

    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut weights = Vec::new();
    let mut n_excluded_missing = 0usize;
    let mut n_excluded_invalid = 0usize;
    let mut n_excluded_missing_weight = 0usize;
    let mut n_excluded_invalid_weight = 0usize;

    for record in &records {
        let outcome_raw = record.get(outcome_index).unwrap_or_default().trim();
        if is_missing_value_for_column(&args.outcome, outcome_raw) {
            n_excluded_missing += 1;
            continue;
        }
        let Ok(outcome_val) = outcome_raw.parse::<f64>() else {
            n_excluded_invalid += 1;
            continue;
        };
        if !outcome_val.is_finite() {
            n_excluded_invalid += 1;
            continue;
        }

        let mut row = vec![1.0]; // intercept
        let mut row_state = RowState::Ok;
        for plan in &variable_plans {
            let value = record.get(plan.source_index).unwrap_or_default();
            match plan.append_design_values(value, &mut row) {
                Ok(()) => {}
                Err(RowState::Ok) => unreachable!(),
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
                x.push(row);
                y.push(outcome_val);
                weights.push(weight);
            }
            RowState::Missing => n_excluded_missing += 1,
            RowState::Invalid => n_excluded_invalid += 1,
        }
    }

    let n_used = y.len();
    if n_used == 0 {
        return Err("No complete analyzable rows remained for linear regression.".to_string());
    }

    let p = design_terms.len(); // number of parameters including intercept
    if n_used <= p {
        return Err(format!(
            "Not enough observations ({n_used}) for {p} parameters. Need at least {} rows.",
            p + 1
        ));
    }

    // OLS: β = (XᵀX)⁻¹ Xᵀy
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for i in 0..n_used {
        let weight = weights[i];
        for j in 0..p {
            xty[j] += x[i][j] * y[i] * weight;
            for k in 0..p {
                xtx[j][k] += x[i][j] * x[i][k] * weight;
            }
        }
    }

    let Ok(xtx_inv) = invert_matrix(&xtx) else {
        return Ok(LinearResult {
            status: "singular".to_string(),
            data_path: path.display().to_string(),
            analysis_path: analysis_path.map(|p| p.display().to_string()),
            formula: build_linear_formula(&args.outcome, &design_terms),
            outcome: args.outcome.clone(),
            predictors: predictors.clone(),
            survey_weight: survey_weight.clone(),
            n_total,
            n_used,
            n_excluded_missing,
            n_excluded_invalid,
            converged: false,
            r_squared: 0.0,
            adjusted_r_squared: 0.0,
            f_statistic: None,
            f_p_value: None,
            residual_std_error: 0.0,
            aic: None,
            bic: None,
            coefficients: Vec::new(),
            notes: vec![
                "Design matrix is singular (XᵀX not invertible); model cannot be fitted."
                    .to_string(),
            ],
            warnings: vec![
                "Singular matrix detected. Check for multicollinearity or constant columns."
                    .to_string(),
            ],
        });
    };

    // Compute β = (XᵀX)⁻¹ Xᵀy
    let mut beta = vec![0.0; p];
    for j in 0..p {
        for k in 0..p {
            beta[j] += xtx_inv[j][k] * xty[k];
        }
    }

    // Compute residuals and RSS
    let weight_total = weights.iter().sum::<f64>();
    let y_mean = y
        .iter()
        .zip(weights.iter())
        .map(|(value, weight)| value * weight)
        .sum::<f64>()
        / weight_total;
    let mut rss = 0.0; // residual sum of squares
    let mut tss = 0.0; // total sum of squares
    for i in 0..n_used {
        let y_hat: f64 = (0..p).map(|j| x[i][j] * beta[j]).sum();
        let residual = y[i] - y_hat;
        rss += weights[i] * residual * residual;
        let deviation = y[i] - y_mean;
        tss += weights[i] * deviation * deviation;
    }

    let r_squared = if tss > 0.0 { 1.0 - rss / tss } else { 0.0 };
    let df_residual = n_used - p;
    let adjusted_r_squared = if df_residual > 0 && tss > 0.0 {
        1.0 - (rss / df_residual as f64) / (tss / (n_used - 1) as f64)
    } else {
        0.0
    };

    let mse = rss / df_residual as f64;
    let residual_std_error = mse.sqrt();

    // F-statistic: (TSS - RSS) / (p-1) / (RSS / (n-p))
    let (f_statistic, f_p_value) = if p > 1 && df_residual > 0 {
        let msr = (tss - rss) / (p - 1) as f64;
        let f_val = msr / mse;
        // Approximate F p-value using the relationship F(1,df) ~ chi-sq
        // For general F, we use a Wilson-Hilferty approximation for large df
        let p_val = f_distribution_p_value(f_val, (p - 1) as f64, df_residual as f64);
        (Some(f_val), Some(p_val))
    } else {
        (None, None)
    };

    // AIC = n * ln(RSS/n) + 2p
    let n_f = n_used as f64;
    let aic = Some(n_f * (rss / n_f).ln() + 2.0 * p as f64);
    let bic = Some(n_f * (rss / n_f).ln() + (p as f64) * n_f.ln());

    // Standard errors: SE(βj) = sqrt(MSE * (XᵀX)⁻¹[j][j])
    let mut coefficients = Vec::new();
    let mut warnings = Vec::new();
    for (term_index, term) in design_terms.iter().enumerate() {
        let se = (mse * xtx_inv[term_index][term_index]).sqrt();
        let t_stat = if se > 0.0 { beta[term_index] / se } else { 0.0 };
        let p_value = if df_residual > 0 {
            t_distribution_p_value(t_stat, df_residual as f64)
        } else {
            1.0
        };
        let t_crit = t_critical_value_95(df_residual as f64);
        let ci_lower = beta[term_index] - t_crit * se;
        let ci_upper = beta[term_index] + t_crit * se;

        coefficients.push(LinearCoefficient {
            term: term.term.clone(),
            variable: term.variable.clone(),
            level: term.level.clone(),
            reference: term.reference.clone(),
            beta: beta[term_index],
            standard_error: se,
            t_statistic: t_stat,
            ci_lower,
            ci_upper,
            p_value,
        });
    }

    if n_used < 10 * p {
        warnings.push(format!(
            "Low observations-per-predictor ratio ({n_used}/{p} = {:.1}). Consider the rule of thumb: at least 10 observations per predictor.",
            n_used as f64 / p as f64
        ));
    }

    let formula = build_linear_formula(&args.outcome, &design_terms);
    let mut notes = vec![
        "Linear regression uses ordinary least squares (OLS).".to_string(),
        format!(
            "Degrees of freedom: model={}, residual={df_residual}.",
            p - 1
        ),
    ];
    if let Some(weight) = &survey_weight {
        notes.push(format!(
            "Survey weight `{weight}` was applied as an observation weight in weighted least squares."
        ));
        notes.push(
            "Complex survey design variance, strata, clusters, and replicate weights are not applied to model standard errors."
                .to_string(),
        );
        notes.push(format!(
            "Excluded {n_excluded_missing_weight} rows with missing `{weight}` and {n_excluded_invalid_weight} rows with invalid/non-positive `{weight}`."
        ));
    }

    Ok(LinearResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|p| p.display().to_string()),
        formula,
        outcome: args.outcome.clone(),
        predictors: predictors.clone(),
        survey_weight,
        n_total,
        n_used,
        n_excluded_missing,
        n_excluded_invalid,
        converged: true,
        r_squared,
        adjusted_r_squared,
        f_statistic,
        f_p_value,
        residual_std_error,
        aic,
        bic,
        coefficients,
        notes,
        warnings,
    })
}

pub(crate) fn build_linear_formula(outcome: &str, terms: &[LogisticTermSpec]) -> String {
    let rhs = terms
        .iter()
        .map(|t| t.term.as_str())
        .collect::<Vec<_>>()
        .join(" + ");
    format!("{outcome} ~ {rhs}")
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_csv(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stats-code-linear-{name}-{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::write(&path, content).expect("write csv");
        path
    }

    fn linear_args(outcome: &str, predictors: &[&str]) -> ModelLinearArgs {
        ModelLinearArgs {
            data: None,
            analysis: None,
            outcome: outcome.to_string(),
            predictors: predictors.iter().map(std::string::ToString::to_string).collect(),
            adjust: Vec::new(),
            strata: Vec::new(),
        }
    }

    /// Test OLS exact solution for y = 2x + 1 (2 data points → exact fit).
    /// With 2 points and 2 parameters (intercept + slope), the system is exactly
    /// determined: β₀ = 1.0, β₁ = 2.0.
    #[test]
    fn ols_2x2_exact_solution() {
        // y = 2x + 1: (0, 1) and (1, 3)
        // With 3 points to satisfy n > p requirement
        let path = temp_csv("ols-exact", "x,y\n0,1\n1,3\n2,5\n");
        let args = linear_args("y", &["x"]);
        let result = linear_csv(&path, None, None, &args).expect("linear regression");

        assert_eq!(result.status, "ok");
        assert!(result.converged);
        assert_eq!(result.coefficients.len(), 2); // intercept + x

        // Intercept should be 1.0
        let intercept = &result.coefficients[0];
        assert!(
            (intercept.beta - 1.0).abs() < 1e-10,
            "intercept beta = {}, expected 1.0",
            intercept.beta
        );

        // Slope should be 2.0
        let slope = &result.coefficients[1];
        assert!(
            (slope.beta - 2.0).abs() < 1e-10,
            "slope beta = {}, expected 2.0",
            slope.beta
        );

        fs::remove_file(path).expect("cleanup");
    }

    /// Test that R² = 1.0 for a perfect linear fit (all points on the line).
    #[test]
    fn perfect_fit_r_squared_is_one() {
        // y = 3x + 2: all points lie exactly on the line
        let path = temp_csv(
            "perfect-fit",
            "x,y\n0,2\n1,5\n2,8\n3,11\n4,14\n",
        );
        let args = linear_args("y", &["x"]);
        let result = linear_csv(&path, None, None, &args).expect("linear regression");

        assert_eq!(result.status, "ok");
        assert!(result.converged);
        assert!(
            (result.r_squared - 1.0).abs() < 1e-10,
            "R² = {}, expected 1.0 for perfect fit",
            result.r_squared
        );
        assert!(
            (result.adjusted_r_squared - 1.0).abs() < 1e-10,
            "Adjusted R² = {}, expected 1.0 for perfect fit",
            result.adjusted_r_squared
        );
        // Residual standard error should be ~0 for perfect fit
        assert!(
            result.residual_std_error < 1e-10,
            "residual_std_error = {}, expected ~0 for perfect fit",
            result.residual_std_error
        );

        fs::remove_file(path).expect("cleanup");
    }

    /// Test that the sum of residuals is approximately zero (a fundamental OLS property).
    /// For OLS with an intercept, Σ(yᵢ - ŷᵢ) = 0 exactly.
    #[test]
    fn residual_sum_approximately_zero() {
        // Use data that does NOT perfectly fit a line, so residuals are non-trivial
        let path = temp_csv(
            "residuals",
            "x,y\n1,2.1\n2,3.9\n3,6.2\n4,7.8\n5,10.1\n6,12.3\n7,13.8\n8,16.2\n",
        );
        let args = linear_args("y", &["x"]);
        let result = linear_csv(&path, None, None, &args).expect("linear regression");

        assert_eq!(result.status, "ok");
        assert!(result.converged);

        // Recompute residuals from the fitted coefficients
        let intercept = result.coefficients[0].beta;
        let slope = result.coefficients[1].beta;
        let ys = [2.1, 3.9, 6.2, 7.8, 10.1, 12.3, 13.8, 16.2];
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];

        let residual_sum: f64 = xs
            .iter()
            .zip(ys.iter())
            .map(|(x, y)| y - (intercept + slope * x))
            .sum();

        assert!(
            residual_sum.abs() < 1e-10,
            "sum of residuals = {residual_sum}, expected ~0"
        );

        // Also verify R² is between 0 and 1 (not perfect fit, not terrible)
        assert!(result.r_squared > 0.99, "R² = {}, expected > 0.99", result.r_squared);
        assert!(result.r_squared <= 1.0, "R² = {}, expected <= 1.0", result.r_squared);

        fs::remove_file(path).expect("cleanup");
    }
}
