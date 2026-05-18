use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::normal_cdf;
use crate::schema::{PoissonCoefficient, PoissonResult};

use super::common::*;

pub(crate) fn poisson_glm_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    outcome_col: &str,
    predictors: &[String],
    offset_col: Option<&str>,
    exposure_col: Option<&str>,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<PoissonResult, String> {
    if offset_col.is_some() && exposure_col.is_some() {
        return Err("--offset and --exposure are mutually exclusive.".to_string());
    }
    let index = column_index(headers);
    let iy = require_column(&index, outcome_col)?;
    let predictor_indices = predictors
        .iter()
        .map(|p| require_column(&index, p).map(|idx| (p.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let ioffset = offset_col.map(|c| require_column(&index, c)).transpose()?;
    let iexposure = exposure_col
        .map(|c| require_column(&index, c))
        .transpose()?;
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut offset = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let raw_y = row.get(iy).unwrap_or("").trim();
        if missing(outcome_col, raw_y) {
            excluded += 1;
            continue;
        }
        let mut row_x = vec![1.0];
        let mut missing_row = false;
        for (name, idx) in &predictor_indices {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                missing_row = true;
                break;
            }
            row_x.push(parse_num(raw, name)?);
        }
        if missing_row {
            excluded += 1;
            continue;
        }
        let off = if let Some(idx) = ioffset {
            let raw = row.get(idx).unwrap_or("").trim();
            if missing(offset_col.unwrap_or("offset"), raw) {
                excluded += 1;
                continue;
            }
            parse_num(raw, offset_col.unwrap_or("offset"))?
        } else if let Some(idx) = iexposure {
            let raw = row.get(idx).unwrap_or("").trim();
            if missing(exposure_col.unwrap_or("exposure"), raw) {
                excluded += 1;
                continue;
            }
            parse_num(raw, exposure_col.unwrap_or("exposure"))?
                .max(EPS)
                .ln()
        } else {
            0.0
        };
        y.push(parse_num(raw_y, outcome_col)?);
        x.push(row_x);
        offset.push(off);
    }
    check_missing_policy(excluded, strategy, "Poisson regression")?;
    let fit =
        crate::math::glm::irls_fit::<crate::math::glm::Poisson>(&x, &y, Some(&offset), 25, 1e-7)?;
    let z = z_critical(alpha);
    let mut coefficients = Vec::new();
    for (idx, beta) in fit.beta.iter().enumerate() {
        let se = fit
            .vcov
            .get(idx)
            .and_then(|row| row.get(idx))
            .copied()
            .unwrap_or(0.0)
            .abs()
            .sqrt();
        let term = if idx == 0 {
            "intercept".to_string()
        } else {
            predictors[idx - 1].clone()
        };
        let irr = beta.exp();
        coefficients.push(PoissonCoefficient {
            term: term.clone(),
            variable: term,
            beta: *beta,
            standard_error: se,
            irr,
            ci_lower: (*beta - z * se).exp(),
            ci_upper: (*beta + z * se).exp(),
            p_value: 2.0 * (1.0 - normal_cdf((*beta / se.max(EPS)).abs())),
        });
    }
    let mut warnings = Vec::new();
    let df = (y.len() as isize - fit.beta.len() as isize).max(1) as f64;
    let dispersion = fit.pearson_chi_square / df;
    if !(0.5..=1.5).contains(&dispersion) {
        warnings.push(format!(
            "dispersion estimate is {dispersion:.3}; check model fit"
        ));
    }
    Ok(PoissonResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: y.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(y.len(), rows.len(), excluded),
        warnings,
        outcome: outcome_col.to_string(),
        predictors: predictors.to_vec(),
        offset: offset_col.or(exposure_col).map(str::to_string),
        offset_kind: if exposure_col.is_some() {
            "raw".to_string()
        } else if offset_col.is_some() {
            "log".to_string()
        } else {
            "none".to_string()
        },
        iterations: fit.iterations,
        converged: fit.converged,
        log_likelihood: fit.log_likelihood,
        deviance: fit.deviance,
        pearson_chi_square: fit.pearson_chi_square,
        aic: -2.0 * fit.log_likelihood + 2.0 * fit.beta.len() as f64,
        coefficients,
    })
}
