use std::collections::BTreeMap;

use crate::cli::NaStrategy;
use crate::helpers::{parse_event_value, require_column};
use crate::math::{chi_square_cdf, quantile_sorted};

pub(super) const EPS: f64 = 1e-12;

pub(super) fn column_index(headers: &csv::StringRecord) -> BTreeMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect()
}

pub(super) fn missing(column: &str, raw: &str) -> bool {
    crate::schema::is_missing_value_for_column(column, raw.trim())
}

pub(super) fn check_missing_policy(
    excluded: usize,
    strategy: NaStrategy,
    context: &str,
) -> Result<(), String> {
    if excluded > 0 && matches!(strategy, NaStrategy::Error) {
        Err(format!(
            "{context} contains {excluded} row(s) with missing values and --na-strategy error was requested."
        ))
    } else {
        Ok(())
    }
}

pub(super) fn parse_num(raw: &str, column: &str) -> Result<f64, String> {
    raw.trim().parse::<f64>().map_err(|_| {
        format!(
            "Column `{column}` contains non-numeric value `{}`.",
            raw.trim()
        )
    })
}

pub(super) fn numeric_column(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    column: &str,
    strategy: NaStrategy,
) -> Result<(Vec<f64>, usize), String> {
    let index = column_index(headers);
    let idx = require_column(&index, column)?;
    let mut values = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let raw = row.get(idx).unwrap_or("").trim();
        if missing(column, raw) {
            excluded += 1;
            continue;
        }
        values.push(parse_num(raw, column)?);
    }
    check_missing_policy(excluded, strategy, column)?;
    Ok((values, excluded))
}

pub(super) fn paired_numeric_columns(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    a: &str,
    b: &str,
    strategy: NaStrategy,
) -> Result<(Vec<(f64, f64)>, usize), String> {
    let index = column_index(headers);
    let ia = require_column(&index, a)?;
    let ib = require_column(&index, b)?;
    let mut pairs = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let ra = row.get(ia).unwrap_or("").trim();
        let rb = row.get(ib).unwrap_or("").trim();
        if missing(a, ra) || missing(b, rb) {
            excluded += 1;
            continue;
        }
        pairs.push((parse_num(ra, a)?, parse_num(rb, b)?));
    }
    check_missing_policy(excluded, strategy, &format!("{a}, {b}"))?;
    Ok((pairs, excluded))
}

pub(super) fn grouped_numeric(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    strategy: NaStrategy,
) -> Result<(BTreeMap<String, Vec<f64>>, usize), String> {
    let index = column_index(headers);
    let iv = require_column(&index, value_col)?;
    let ig = require_column(&index, group_col)?;
    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in rows {
        let raw_v = row.get(iv).unwrap_or("").trim();
        let raw_g = row.get(ig).unwrap_or("").trim();
        if missing(value_col, raw_v) || missing(group_col, raw_g) {
            excluded += 1;
            continue;
        }
        groups
            .entry(raw_g.to_string())
            .or_default()
            .push(parse_num(raw_v, value_col)?);
    }
    check_missing_policy(excluded, strategy, &format!("{value_col}, {group_col}"))?;
    Ok((groups, excluded))
}

pub(super) fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

pub(super) fn sample_variance(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() as f64 - 1.0)
}

pub(super) fn sample_sd(values: &[f64]) -> f64 {
    sample_variance(values).sqrt()
}

pub(super) fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    quantile_sorted(&sorted, 0.5)
}

pub(super) fn z_critical(alpha: f64) -> f64 {
    inverse_normal_cdf(1.0 - alpha / 2.0)
}

pub(super) fn inverse_normal_cdf(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let a = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    let b = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    let c = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    let d = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let plow = 0.02425;
    let phigh = 1.0 - plow;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    } else if p <= phigh {
        let q = p - 0.5;
        let r = q * q;
        (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    }
}

pub(super) fn chi_square_p_value(x: f64, df: f64) -> f64 {
    (1.0 - chi_square_cdf(x, df)).clamp(0.0, 1.0)
}

pub(super) fn rank_with_ties(values: &[f64]) -> Vec<f64> {
    crate::math::rank_with_ties_by_tolerance(values, EPS)
}

pub(super) fn event_value(raw: &str, column: &str, override_value: Option<&str>) -> Option<bool> {
    let trimmed = raw.trim();
    if missing(column, trimmed) {
        return None;
    }
    if let Some(expected) = override_value {
        return Some(trimmed == expected);
    }
    parse_event_value(trimmed).map(|value| value != 0.0)
}

pub(super) fn prelude_notes(n_used: usize, n_total: usize, excluded: usize) -> Vec<String> {
    vec![format!(
        "Used {n_used} of {n_total} rows; excluded {excluded} row(s) with missing required values."
    )]
}

pub(super) fn stringify_csv_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
