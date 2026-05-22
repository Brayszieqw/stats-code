use std::collections::BTreeMap;

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::normal_cdf;
use crate::schema::{DoseResponseCategory, DoseResponseResult};

use super::common::{
    check_missing_policy, column_index, missing, parse_num, prelude_notes, z_critical, EPS,
};

pub(crate) fn dose_response_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    exposure_col: &str,
    outcome_col: &str,
    person_time_col: &str,
    scores: &[f64],
    alpha: f64,
    strategy: NaStrategy,
) -> Result<DoseResponseResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, exposure_col)?;
    let io = require_column(&index, outcome_col)?;
    let ipt = require_column(&index, person_time_col)?;
    let mut agg: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let ro = row.get(io).unwrap_or("").trim();
        let rpt = row.get(ipt).unwrap_or("").trim();
        if missing(exposure_col, re) || missing(outcome_col, ro) || missing(person_time_col, rpt) {
            excluded += 1;
            continue;
        }
        let events = parse_num(ro, outcome_col)?;
        let pt = parse_num(rpt, person_time_col)?;
        if pt <= 0.0 {
            excluded += 1;
            continue;
        }
        let entry = agg.entry(re.to_string()).or_default();
        entry.0 += events.round().max(0.0) as usize;
        entry.1 += pt;
    }
    check_missing_policy(excluded, strategy, "dose-response")?;
    if agg.len() < 2 {
        return Err(
            "Dose-response analysis requires at least two exposure categories.".to_string(),
        );
    }
    if !scores.is_empty() && scores.len() != agg.len() {
        return Err("--scores length must match the number of exposure categories.".to_string());
    }
    let z = z_critical(alpha);
    let ref_rate = agg
        .values()
        .next()
        .map_or(1.0, |(events, pt)| *events as f64 / pt.max(EPS))
        .max(EPS);
    let mut categories = Vec::new();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut ws = Vec::new();
    for (idx, (label, (events, pt))) in agg.iter().enumerate() {
        let score = scores.get(idx).copied().unwrap_or(idx as f64);
        let rate = *events as f64 / pt.max(EPS);
        let se_log = 1.0 / (*events as f64).max(1.0).sqrt();
        categories.push(DoseResponseCategory {
            category: label.clone(),
            score,
            events: *events,
            person_time: *pt,
            rate,
            rate_ratio: rate / ref_rate,
            rr_ci_lower: ((rate / ref_rate).ln() - z * se_log).exp(),
            rr_ci_upper: ((rate / ref_rate).ln() + z * se_log).exp(),
        });
        xs.push(score);
        ys.push(rate.max(EPS).ln());
        ws.push((*events as f64).max(1.0));
    }
    let (beta, se) = weighted_slope(&xs, &ys, &ws);
    let trend_z = beta / se.max(EPS);
    Ok(DoseResponseResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: rows.len() - excluded,
        n_excluded_missing: excluded,
        notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
        warnings: vec![],
        exposure: exposure_col.to_string(),
        outcome: outcome_col.to_string(),
        categories,
        trend_beta: beta,
        trend_se: se,
        trend_ci_lower: beta - z * se,
        trend_ci_upper: beta + z * se,
        trend_p_value: 2.0 * (1.0 - normal_cdf(trend_z.abs())),
        linearity_p_value: 1.0,
    })
}

fn weighted_slope(x: &[f64], y: &[f64], w: &[f64]) -> (f64, f64) {
    let sw = w.iter().sum::<f64>().max(EPS);
    let mx = x.iter().zip(w).map(|(x, w)| x * w).sum::<f64>() / sw;
    let my = y.iter().zip(w).map(|(y, w)| y * w).sum::<f64>() / sw;
    let sxx = x
        .iter()
        .zip(w)
        .map(|(x, w)| w * (x - mx).powi(2))
        .sum::<f64>()
        .max(EPS);
    let sxy = x
        .iter()
        .zip(y)
        .zip(w)
        .map(|((x, y), w)| w * (x - mx) * (y - my))
        .sum::<f64>();
    let beta = sxy / sxx;
    let residual = x
        .iter()
        .zip(y)
        .zip(w)
        .map(|((x, y), w)| {
            let fitted = my + beta * (x - mx);
            w * (y - fitted).powi(2)
        })
        .sum::<f64>();
    let se = (residual / ((x.len() as f64 - 2.0).max(1.0)) / sxx).sqrt();
    (beta, se)
}
