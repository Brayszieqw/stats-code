use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::normal_cdf;
use crate::schema::{MannWhitneyResult, McNemarResult, WilcoxonSignedRankResult};

use super::common::*;

pub(crate) fn mcnemar_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    var1: &str,
    var2: &str,
    exact_threshold: usize,
    strategy: NaStrategy,
) -> Result<McNemarResult, String> {
    let index = column_index(headers);
    let i1 = require_column(&index, var1)?;
    let i2 = require_column(&index, var2)?;
    let mut b = 0usize;
    let mut c = 0usize;
    let mut concordant = 0usize;
    let mut excluded = 0usize;
    for row in rows {
        let r1 = row.get(i1).unwrap_or("").trim();
        let r2 = row.get(i2).unwrap_or("").trim();
        let Some(a) = event_value(r1, var1, None) else {
            excluded += 1;
            continue;
        };
        let Some(d) = event_value(r2, var2, None) else {
            excluded += 1;
            continue;
        };
        match (a, d) {
            (true, false) => b += 1,
            (false, true) => c += 1,
            _ => concordant += 1,
        }
    }
    check_missing_policy(excluded, strategy, "McNemar test")?;
    let discordant = b + c;
    if discordant == 0 {
        return Err("McNemar test has no discordant pairs.".to_string());
    }
    let chi_square = ((b as f64 - c as f64).abs() - 1.0).max(0.0).powi(2) / discordant as f64;
    let p_value = chi_square_p_value(chi_square, 1.0);
    let exact_p_value = if discordant < exact_threshold {
        let k = b.min(c);
        Some((2.0 * binomial_cdf(k, discordant, 0.5)).min(1.0))
    } else {
        None
    };
    Ok(McNemarResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: rows.len() - excluded,
        n_excluded_missing: excluded,
        notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
        warnings: vec![],
        var1: var1.to_string(),
        var2: var2.to_string(),
        b,
        c,
        n_concordant: concordant,
        chi_square,
        continuity_correction_used: true,
        p_value,
        exact_p_value,
    })
}

fn binomial_cdf(k: usize, n: usize, p: f64) -> f64 {
    let mut sum = 0.0;
    for i in 0..=k {
        sum += (ln_choose(n, i) + (i as f64) * p.ln() + ((n - i) as f64) * (1.0 - p).ln()).exp();
    }
    sum
}

fn ln_choose(n: usize, k: usize) -> f64 {
    crate::math::log_gamma_lanczos((n + 1) as f64)
        - crate::math::log_gamma_lanczos((k + 1) as f64)
        - crate::math::log_gamma_lanczos((n - k + 1) as f64)
}

pub(crate) fn wilcoxon_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    var1: &str,
    var2: &str,
    strategy: NaStrategy,
) -> Result<WilcoxonSignedRankResult, String> {
    let (pairs, excluded_missing) = paired_numeric_columns(rows, headers, var1, var2, strategy)?;
    let mut diffs = Vec::new();
    let mut zeros = 0usize;
    for (a, b) in pairs {
        let d = b - a;
        if d.abs() < EPS {
            zeros += 1;
        } else {
            diffs.push(d);
        }
    }
    if diffs.is_empty() {
        return Err("no non-zero paired differences".to_string());
    }
    let abs: Vec<f64> = diffs.iter().map(|d| d.abs()).collect();
    let ranks = rank_with_ties(&abs);
    let w_plus = diffs
        .iter()
        .zip(ranks.iter())
        .filter_map(|(d, r)| if *d > 0.0 { Some(*r) } else { None })
        .sum::<f64>();
    let n = diffs.len() as f64;
    let expected_w = n * (n + 1.0) / 4.0;
    let variance_w = n * (n + 1.0) * (2.0 * n + 1.0) / 24.0;
    let z_statistic =
        (w_plus - expected_w - 0.5 * (w_plus - expected_w).signum()) / variance_w.sqrt().max(EPS);
    let p_value = 2.0 * (1.0 - normal_cdf(z_statistic.abs()));
    Ok(WilcoxonSignedRankResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: diffs.len(),
        n_excluded_missing: excluded_missing,
        notes: prelude_notes(diffs.len(), rows.len(), excluded_missing),
        warnings: vec![],
        var1: var1.to_string(),
        var2: var2.to_string(),
        w_plus,
        expected_w,
        variance_w,
        z_statistic,
        p_value,
        n_zero_pairs_excluded: zeros,
        n_ties_corrected: count_tie_groups(&abs),
    })
}

fn count_tie_groups(values: &[f64]) -> usize {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut count = 0usize;
    let mut i = 0usize;
    while i < sorted.len() {
        let mut j = i + 1;
        while j < sorted.len() && (sorted[j] - sorted[i]).abs() < EPS {
            j += 1;
        }
        if j - i > 1 {
            count += 1;
        }
        i = j;
    }
    count
}

pub(crate) fn mann_whitney_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    strategy: NaStrategy,
) -> Result<MannWhitneyResult, String> {
    let (groups, excluded) = grouped_numeric(rows, headers, value_col, group_col, strategy)?;
    if groups.len() != 2 {
        return Err(format!(
            "Mann-Whitney U requires exactly 2 groups; found {}. Kruskal-Wallis is not in the current spec scope.",
            groups.len()
        ));
    }
    let labels: Vec<String> = groups.keys().cloned().collect();
    let a = groups.get(&labels[0]).unwrap();
    let b = groups.get(&labels[1]).unwrap();
    let mut pooled = Vec::new();
    for v in a {
        pooled.push((*v, 0usize));
    }
    for v in b {
        pooled.push((*v, 1usize));
    }
    let values: Vec<f64> = pooled.iter().map(|(v, _)| *v).collect();
    let ranks = rank_with_ties(&values);
    let rank_a: f64 = pooled
        .iter()
        .zip(ranks.iter())
        .filter_map(|((_, g), r)| if *g == 0 { Some(*r) } else { None })
        .sum();
    let n_a = a.len();
    let n_b = b.len();
    let u_a = rank_a - n_a as f64 * (n_a as f64 + 1.0) / 2.0;
    let u_b = n_a as f64 * n_b as f64 - u_a;
    let u = u_a.min(u_b);
    let mean_u = n_a as f64 * n_b as f64 / 2.0;
    let sd_u = (n_a as f64 * n_b as f64 * (n_a + n_b + 1) as f64 / 12.0).sqrt();
    let z = (u - mean_u) / sd_u.max(EPS);
    Ok(MannWhitneyResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n_a + n_b,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_a + n_b, rows.len(), excluded),
        warnings: vec![],
        variable: value_col.to_string(),
        group: group_col.to_string(),
        group_a_label: labels[0].clone(),
        group_b_label: labels[1].clone(),
        n_a,
        n_b,
        median_a: median(a),
        median_b: median(b),
        u_statistic: u,
        z_statistic: z,
        p_value: 2.0 * (1.0 - normal_cdf(z.abs())),
    })
}
