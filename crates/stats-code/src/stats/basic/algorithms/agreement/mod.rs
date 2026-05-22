use std::collections::{BTreeMap, BTreeSet};

use crate::cli::{AgreementKappaArgs, NaStrategy};
use crate::helpers::require_column;
use crate::math::t_distribution_critical_value;
use crate::schema::{BlandAltmanPoint, BlandAltmanResult, KappaResult};

use super::common::{
    check_missing_policy, column_index, mean, missing, paired_numeric_columns, prelude_notes,
    sample_sd, z_critical, EPS,
};

pub(crate) fn kappa_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    args: &AgreementKappaArgs,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<KappaResult, String> {
    let index = column_index(headers);
    let i1 = require_column(&index, &args.rater1)?;
    let i2 = require_column(&index, &args.rater2)?;
    let mut pairs = Vec::new();
    let mut categories = BTreeSet::new();
    let mut excluded = 0usize;
    for row in rows {
        let a = row.get(i1).unwrap_or("").trim();
        let b = row.get(i2).unwrap_or("").trim();
        if missing(&args.rater1, a) || missing(&args.rater2, b) {
            excluded += 1;
            continue;
        }
        categories.insert(a.to_string());
        categories.insert(b.to_string());
        pairs.push((a.to_string(), b.to_string()));
    }
    check_missing_policy(excluded, strategy, "kappa")?;
    if pairs.is_empty() {
        return Err("Kappa requires at least one complete rating pair.".to_string());
    }
    let cats: Vec<String> = categories.into_iter().collect();
    let lookup: BTreeMap<String, usize> = cats
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), i))
        .collect();
    let k = cats.len();
    let mut matrix = vec![vec![0usize; k]; k];
    for (a, b) in &pairs {
        matrix[lookup[a]][lookup[b]] += 1;
    }
    let n = pairs.len() as f64;
    let observed = (0..k).map(|i| matrix[i][i] as f64).sum::<f64>() / n;
    let mut expected = 0.0;
    for i in 0..k {
        let row_sum: usize = matrix[i].iter().sum();
        let col_sum: usize = matrix.iter().map(|row| row[i]).sum();
        expected += row_sum as f64 * col_sum as f64;
    }
    expected /= n * n;
    let kappa = (observed - expected) / (1.0 - expected).max(EPS);
    let se = ((observed * (1.0 - observed)) / (n * (1.0 - expected).powi(2)).max(EPS)).sqrt();
    let z = z_critical(alpha);
    let weighted = if args.weights.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(weighted_kappa(&matrix, &args.weights))
    };
    Ok(KappaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: pairs.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(pairs.len(), rows.len(), excluded),
        warnings: vec![],
        rater1: args.rater1.clone(),
        rater2: args.rater2.clone(),
        categories: cats,
        agreement_matrix: matrix,
        observed_agreement: observed,
        expected_agreement: expected,
        kappa,
        kappa_se: se,
        kappa_ci_lower: kappa - z * se,
        kappa_ci_upper: kappa + z * se,
        weighted_kappa: weighted,
        weights_kind: args.weights.clone(),
    })
}

fn weighted_kappa(matrix: &[Vec<usize>], weights: &str) -> f64 {
    let k = matrix.len();
    if k <= 1 {
        return 1.0;
    }
    let n: f64 = matrix.iter().flatten().map(|v| *v as f64).sum();
    let mut row = vec![0.0; k];
    let mut col = vec![0.0; k];
    for i in 0..k {
        for j in 0..k {
            row[i] += matrix[i][j] as f64;
            col[j] += matrix[i][j] as f64;
        }
    }
    let mut obs = 0.0;
    let mut exp = 0.0;
    for i in 0..k {
        for j in 0..k {
            let dist = (i as f64 - j as f64).abs() / (k as f64 - 1.0);
            let w = if weights.eq_ignore_ascii_case("quadratic") {
                dist.powi(2)
            } else {
                dist
            };
            obs += w * matrix[i][j] as f64 / n.max(EPS);
            exp += w * row[i] * col[j] / n.powi(2).max(EPS);
        }
    }
    1.0 - obs / exp.max(EPS)
}

pub(crate) fn bland_altman_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    method1: &str,
    method2: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<BlandAltmanResult, String> {
    let (pairs, excluded) = paired_numeric_columns(rows, headers, method1, method2, strategy)?;
    if pairs.len() < 2 {
        return Err("Bland-Altman analysis requires at least two complete pairs.".to_string());
    }
    let diffs: Vec<f64> = pairs.iter().map(|(a, b)| b - a).collect();
    let points: Vec<BlandAltmanPoint> = pairs
        .iter()
        .map(|(a, b)| BlandAltmanPoint {
            mean: (a + b) / 2.0,
            diff: b - a,
        })
        .collect();
    let n = diffs.len();
    let bias = mean(&diffs);
    let sd = sample_sd(&diffs);
    let z = z_critical(alpha);
    let t = t_distribution_critical_value(alpha, (n - 1) as f64);
    let se_bias = sd / (n as f64).sqrt();
    let loa_lower = bias - z * sd;
    let loa_upper = bias + z * sd;
    let se_loa = sd * (1.0 / n as f64 + z.powi(2) / (2.0 * (n - 1) as f64)).sqrt();
    let n_outside = diffs
        .iter()
        .filter(|d| **d < loa_lower || **d > loa_upper)
        .count();
    let mut warnings = Vec::new();
    if n < 10 {
        warnings.push("Bland-Altman limits of agreement are unstable when n < 10.".to_string());
    }
    Ok(BlandAltmanResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n,
        n_excluded_missing: excluded,
        notes: prelude_notes(n, rows.len(), excluded),
        warnings,
        method1: method1.to_string(),
        method2: method2.to_string(),
        n,
        bias,
        bias_ci_lower: bias - t * se_bias,
        bias_ci_upper: bias + t * se_bias,
        sd_difference: sd,
        loa_lower,
        loa_upper,
        loa_lower_ci_lower: loa_lower - t * se_loa,
        loa_lower_ci_upper: loa_lower + t * se_loa,
        loa_upper_ci_lower: loa_upper - t * se_loa,
        loa_upper_ci_upper: loa_upper + t * se_loa,
        n_outside_loa: n_outside,
        points,
    })
}
