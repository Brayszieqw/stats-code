use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::schema::AttributableRiskResult;

use super::common::*;

pub(crate) fn attributable_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    exposure_col: &str,
    outcome_col: &str,
    person_time_col: Option<&str>,
    exposure_prevalence: Option<f64>,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<AttributableRiskResult, String> {
    if let Some(prevalence) = exposure_prevalence {
        if !(0.0..=1.0).contains(&prevalence) {
            return Err("--exposure-prevalence must be between 0 and 1.".to_string());
        }
    }
    let index = column_index(headers);
    let ie = require_column(&index, exposure_col)?;
    let io = require_column(&index, outcome_col)?;
    let ipt = person_time_col
        .map(|c| require_column(&index, c))
        .transpose()?;
    let mut exp_events = 0.0;
    let mut exp_pt = 0.0;
    let mut unexp_events = 0.0;
    let mut unexp_pt = 0.0;
    let mut exposed_n = 0usize;
    let mut n_used = 0usize;
    let mut excluded = 0usize;
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let ro = row.get(io).unwrap_or("").trim();
        let Some(exposed) = event_value(re, exposure_col, None) else {
            excluded += 1;
            continue;
        };
        let Some(event) = event_value(ro, outcome_col, None) else {
            excluded += 1;
            continue;
        };
        let pt = if let Some(idx) = ipt {
            let raw = row.get(idx).unwrap_or("").trim();
            if missing(person_time_col.unwrap_or("person_time"), raw) {
                excluded += 1;
                continue;
            }
            parse_num(raw, person_time_col.unwrap_or("person_time"))?
        } else {
            1.0
        };
        n_used += 1;
        if exposed {
            exposed_n += 1;
            exp_pt += pt;
            if event {
                exp_events += 1.0;
            }
        } else {
            unexp_pt += pt;
            if event {
                unexp_events += 1.0;
            }
        }
    }
    check_missing_policy(excluded, strategy, "attributable risk")?;
    let rate_exposed = exp_events / exp_pt.max(EPS);
    let rate_unexposed = unexp_events / unexp_pt.max(EPS);
    let ar = rate_exposed - rate_unexposed;
    let z = z_critical(alpha);
    let se_ar = (exp_events.max(1.0) / exp_pt.powi(2).max(EPS)
        + unexp_events.max(1.0) / unexp_pt.powi(2).max(EPS))
    .sqrt();
    let prevalence = exposure_prevalence.or_else(|| {
        if n_used > 0 {
            Some(exposed_n as f64 / n_used as f64)
        } else {
            None
        }
    });
    let par = prevalence.map(|p| {
        let value = p * ar;
        let se = p.abs() * se_ar;
        let target_rate = rate_unexposed + value;
        (value, se, target_rate)
    });
    let mut warnings = Vec::new();
    if rate_unexposed > rate_exposed {
        warnings.push("protective association detected".to_string());
    }
    Ok(AttributableRiskResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_used, rows.len(), excluded),
        warnings,
        exposure: exposure_col.to_string(),
        outcome: outcome_col.to_string(),
        rate_exposed,
        rate_unexposed,
        ar,
        ar_ci_lower: ar - z * se_ar,
        ar_ci_upper: ar + z * se_ar,
        ar_percent: if rate_exposed.abs() > EPS {
            ar / rate_exposed * 100.0
        } else {
            f64::NAN
        },
        par: par.map(|(value, _, _)| value),
        par_ci_lower: par.map(|(value, se, _)| value - z * se),
        par_ci_upper: par.map(|(value, se, _)| value + z * se),
        par_percent: par.map(|(value, _, target_rate)| value / target_rate.max(EPS) * 100.0),
        exposure_prevalence: prevalence,
    })
}
