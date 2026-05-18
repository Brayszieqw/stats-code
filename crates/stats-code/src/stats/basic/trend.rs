use std::collections::BTreeMap;

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::normal_cdf;
use crate::schema::{CategoryProportion, CochranArmitageResult};

use super::common::*;

pub(crate) fn cochran_armitage_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    exposure_col: &str,
    outcome_col: &str,
    scores: &[f64],
    strategy: NaStrategy,
) -> Result<CochranArmitageResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, exposure_col)?;
    let io = require_column(&index, outcome_col)?;
    let mut table: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let ro = row.get(io).unwrap_or("").trim();
        if missing(exposure_col, re) {
            excluded += 1;
            continue;
        }
        let Some(event) = event_value(ro, outcome_col, None) else {
            excluded += 1;
            continue;
        };
        let entry = table.entry(re.to_string()).or_default();
        entry.0 += 1;
        if event {
            entry.1 += 1;
        }
    }
    check_missing_policy(excluded, strategy, "Cochran-Armitage trend test")?;
    if table.len() < 2 {
        return Err(
            "Cochran-Armitage trend test requires at least two ordered categories.".to_string(),
        );
    }
    if !scores.is_empty() && scores.len() != table.len() {
        return Err(format!(
            "--scores length ({}) must match number of ordered categories ({}).",
            scores.len(),
            table.len()
        ));
    }
    if table.values().filter(|(_, events)| *events > 0).count() < 2 {
        return Err(
            "Cochran-Armitage trend test requires events in at least two ordered categories."
                .to_string(),
        );
    }
    let mut categories = Vec::new();
    let n_total_used: usize = table.values().map(|(n, _)| *n).sum();
    let total_events: usize = table.values().map(|(_, e)| *e).sum();
    let p = total_events as f64 / n_total_used as f64;
    let mut sum_s_events_minus_expected = 0.0;
    let mut sum_ns = 0.0;
    let mut sum_ns2 = 0.0;
    for (idx, (label, (n, events))) in table.iter().enumerate() {
        let score = scores.get(idx).copied().unwrap_or(idx as f64);
        categories.push(CategoryProportion {
            category: label.clone(),
            score,
            n: *n,
            events: *events,
            proportion: *events as f64 / *n as f64,
        });
        sum_s_events_minus_expected += score * (*events as f64 - *n as f64 * p);
        sum_ns += *n as f64 * score;
        sum_ns2 += *n as f64 * score * score;
    }
    let var = p * (1.0 - p) * (sum_ns2 - sum_ns.powi(2) / n_total_used as f64);
    let z = sum_s_events_minus_expected / var.sqrt().max(EPS);
    Ok(CochranArmitageResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n_total_used,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_total_used, rows.len(), excluded),
        warnings: vec![],
        exposure: exposure_col.to_string(),
        outcome: outcome_col.to_string(),
        categories,
        trend_statistic: z,
        p_value: 2.0 * (1.0 - normal_cdf(z.abs())),
    })
}
