use std::collections::{BTreeMap, BTreeSet};

use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::distributions::studentized_range_p;
use crate::math::{
    chi_square_cdf, f_distribution_p_value, helmert_contrast_matrix, jacobi_eigh,
    matrix_determinant, matrix_multiply, matrix_trace, t_distribution_critical_value,
    t_distribution_p_value,
};
use crate::schema::{
    AnovaGroupSummary, OneWayAnovaResult, PosthocPair, PosthocResult, RbdAnovaResult,
    RepeatedAnovaResult,
};

use super::common::*;

pub(crate) fn oneway_anova_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    strategy: NaStrategy,
) -> Result<OneWayAnovaResult, String> {
    let (groups, excluded) = grouped_numeric(rows, headers, value_col, group_col, strategy)?;
    if groups.len() < 2 {
        return Err("One-way ANOVA requires at least two groups.".to_string());
    }
    for (label, values) in &groups {
        if values.len() < 2 {
            return Err(format!(
                "One-way ANOVA requires at least 2 observations per group; group `{label}` has {}.",
                values.len()
            ));
        }
    }
    let all: Vec<f64> = groups.values().flatten().copied().collect();
    let n_used = all.len();
    let overall_mean = mean(&all);
    let mut summaries = Vec::new();
    let mut ss_between = 0.0;
    let mut ss_within = 0.0;
    for (label, values) in &groups {
        let group_mean = mean(values);
        let sd = sample_sd(values);
        summaries.push(AnovaGroupSummary {
            group: label.clone(),
            n: values.len(),
            mean: group_mean,
            sd,
        });
        ss_between += values.len() as f64 * (group_mean - overall_mean).powi(2);
        ss_within += values.iter().map(|v| (v - group_mean).powi(2)).sum::<f64>();
    }
    let ss_total = all.iter().map(|v| (v - overall_mean).powi(2)).sum::<f64>();
    let df_between = groups.len() - 1;
    let df_within = n_used - groups.len();
    let ms_between = ss_between / df_between as f64;
    let ms_within = ss_within / df_within as f64;
    let f_statistic = ms_between / ms_within.max(EPS);
    let p_value = f_distribution_p_value(f_statistic, df_between as f64, df_within as f64);
    Ok(OneWayAnovaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_used, rows.len(), excluded),
        warnings: vec![],
        variable: value_col.to_string(),
        group: group_col.to_string(),
        overall_mean,
        groups: summaries,
        ss_between,
        ss_within,
        ss_total,
        df_between,
        df_within,
        ms_between,
        ms_within,
        f_statistic,
        p_value,
    })
}

pub(crate) fn rbd_anova_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    block_col: &str,
    strategy: NaStrategy,
) -> Result<RbdAnovaResult, String> {
    let index = column_index(headers);
    let iv = require_column(&index, value_col)?;
    let ig = require_column(&index, group_col)?;
    let ib = require_column(&index, block_col)?;
    let mut records = Vec::new();
    let mut excluded = 0usize;
    let mut groups = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    for row in rows {
        let rv = row.get(iv).unwrap_or("").trim();
        let rg = row.get(ig).unwrap_or("").trim();
        let rb = row.get(ib).unwrap_or("").trim();
        if missing(value_col, rv) || missing(group_col, rg) || missing(block_col, rb) {
            excluded += 1;
            continue;
        }
        let value = parse_num(rv, value_col)?;
        let group = rg.to_string();
        let block = rb.to_string();
        groups.insert(group.clone());
        blocks.insert(block.clone());
        records.push((group, block, value));
    }
    check_missing_policy(excluded, strategy, "randomized-block ANOVA")?;
    if groups.len() < 2 || blocks.len() < 2 {
        return Err(
            "Randomized-block ANOVA requires at least two treatments and two blocks.".to_string(),
        );
    }
    let n = records.len();
    let grand = records.iter().map(|(_, _, v)| *v).sum::<f64>() / n as f64;
    let mut group_values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut block_values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (g, b, v) in &records {
        group_values.entry(g.clone()).or_default().push(*v);
        block_values.entry(b.clone()).or_default().push(*v);
    }
    let ss_total = records
        .iter()
        .map(|(_, _, v)| (v - grand).powi(2))
        .sum::<f64>();
    let ss_group = group_values
        .values()
        .map(|values| values.len() as f64 * (mean(values) - grand).powi(2))
        .sum::<f64>();
    let ss_block = block_values
        .values()
        .map(|values| values.len() as f64 * (mean(values) - grand).powi(2))
        .sum::<f64>();
    let ss_error = (ss_total - ss_group - ss_block).max(0.0);
    let treatment_df1 = groups.len() - 1;
    let block_df1 = blocks.len() - 1;
    let treatment_df2 = n.saturating_sub(groups.len() + blocks.len() - 1);
    if treatment_df2 == 0 {
        return Err("Randomized-block ANOVA has zero residual degrees of freedom.".to_string());
    }
    let error_ms = ss_error / treatment_df2 as f64;
    let treatment_f = (ss_group / treatment_df1 as f64) / error_ms.max(EPS);
    let block_f = (ss_block / block_df1 as f64) / error_ms.max(EPS);
    Ok(RbdAnovaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n,
        n_excluded_missing: excluded,
        notes: prelude_notes(n, rows.len(), excluded),
        warnings: vec![],
        variable: value_col.to_string(),
        group: group_col.to_string(),
        block: block_col.to_string(),
        treatment_f,
        treatment_df1,
        treatment_df2,
        treatment_p: f_distribution_p_value(
            treatment_f,
            treatment_df1 as f64,
            treatment_df2 as f64,
        ),
        block_f,
        block_df1,
        block_df2: treatment_df2,
        block_p: f_distribution_p_value(block_f, block_df1 as f64, treatment_df2 as f64),
        error_ms,
    })
}

pub(crate) fn posthoc_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    method: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<PosthocResult, String> {
    let anova = oneway_anova_csv(rows, headers, value_col, group_col, strategy)?;
    let m = anova.groups.len() * (anova.groups.len() - 1) / 2;
    let mut pairs = Vec::new();
    let tcrit = t_distribution_critical_value(alpha / m.max(1) as f64, anova.df_within as f64);
    for i in 0..anova.groups.len() {
        for j in (i + 1)..anova.groups.len() {
            let a = &anova.groups[i];
            let b = &anova.groups[j];
            let diff = a.mean - b.mean;
            let se = (anova.ms_within * (1.0 / a.n as f64 + 1.0 / b.n as f64)).sqrt();
            let stat = diff / se.max(EPS);
            let raw_p = t_distribution_p_value(stat, anova.df_within as f64);
            let adjusted = if method.eq_ignore_ascii_case("tukey") {
                let q = stat.abs() * 2.0_f64.sqrt();
                studentized_range_p(q, anova.groups.len(), anova.df_within as f64)
            } else {
                (raw_p * m as f64).min(1.0)
            };
            pairs.push(PosthocPair {
                group_a: a.group.clone(),
                group_b: b.group.clone(),
                mean_difference: diff,
                standard_error: se,
                test_statistic: stat,
                adjusted_p_value: adjusted,
                ci_lower: diff - tcrit * se,
                ci_upper: diff + tcrit * se,
            });
        }
    }
    Ok(PosthocResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: anova.n_total,
        n_used: anova.n_used,
        n_excluded_missing: anova.n_excluded_missing,
        notes: anova.notes,
        warnings: if method.eq_ignore_ascii_case("tukey") {
            vec![
                "Tukey HSD uses the built-in conservative approximation in this release."
                    .to_string(),
            ]
        } else {
            vec![]
        },
        variable: value_col.to_string(),
        group: group_col.to_string(),
        method: method.to_ascii_lowercase(),
        pairs,
    })
}

pub(crate) fn repeated_anova_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    subject_col: &str,
    time_col: &str,
    strategy: NaStrategy,
) -> Result<RepeatedAnovaResult, String> {
    let index = column_index(headers);
    let iv = require_column(&index, value_col)?;
    let isub = require_column(&index, subject_col)?;
    let itime = require_column(&index, time_col)?;
    let mut raw: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    let mut times = BTreeSet::new();
    let mut excluded = 0usize;
    for row in rows {
        let rv = row.get(iv).unwrap_or("").trim();
        let rs = row.get(isub).unwrap_or("").trim();
        let rt = row.get(itime).unwrap_or("").trim();
        if missing(value_col, rv) || missing(subject_col, rs) || missing(time_col, rt) {
            excluded += 1;
            continue;
        }
        let value = parse_num(rv, value_col)?;
        times.insert(rt.to_string());
        raw.entry(rs.to_string())
            .or_default()
            .insert(rt.to_string(), value);
    }
    check_missing_policy(excluded, strategy, "repeated-measures ANOVA")?;
    let time_levels: Vec<String> = times.into_iter().collect();
    if time_levels.len() < 2 {
        return Err("Repeated-measures ANOVA requires at least two time points.".to_string());
    }
    let mut matrix = Vec::new();
    for values_by_time in raw.values() {
        if time_levels.iter().all(|t| values_by_time.contains_key(t)) {
            matrix.push(
                time_levels
                    .iter()
                    .map(|t| *values_by_time.get(t).unwrap_or(&0.0))
                    .collect::<Vec<_>>(),
            );
        } else {
            excluded += 1;
        }
    }
    let n_subjects = matrix.len();
    let n_timepoints = time_levels.len();
    if n_subjects < 2 {
        return Err("Repeated-measures ANOVA requires at least two complete subjects.".to_string());
    }
    let values: Vec<f64> = matrix.iter().flatten().copied().collect();
    let grand = mean(&values);
    let subject_means: Vec<f64> = matrix.iter().map(|row| mean(row)).collect();
    let mut time_means = vec![0.0; n_timepoints];
    for t in 0..n_timepoints {
        time_means[t] = matrix.iter().map(|row| row[t]).sum::<f64>() / n_subjects as f64;
    }
    let ss_total = values.iter().map(|v| (v - grand).powi(2)).sum::<f64>();
    let ss_subject = n_timepoints as f64
        * subject_means
            .iter()
            .map(|m| (m - grand).powi(2))
            .sum::<f64>();
    let ss_time = n_subjects as f64 * time_means.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let ss_error = (ss_total - ss_subject - ss_time).max(0.0);
    let df_time = n_timepoints - 1;
    let df_error = (n_subjects - 1) * df_time;
    let time_f = (ss_time / df_time as f64) / (ss_error / df_error as f64).max(EPS);
    let time_p = f_distribution_p_value(time_f, df_time as f64, df_error as f64);

    // Mauchly's test of sphericity + GG/HF corrections
    let (mauchly_w, mauchly_p, gg_epsilon, hf_epsilon, gg_p, hf_p) =
        mauchly_and_corrections(&matrix, n_subjects, n_timepoints, time_f, df_time, df_error);

    let mut warnings = Vec::new();
    if let Some(mauchly_p) = mauchly_p {
        if mauchly_p < 0.05 {
            warnings.push(format!(
                "Mauchly test of sphericity is significant (W={:.4}, p={:.4}). Greenhouse-Geisser and Huynh-Feldt corrected p-values are provided.",
                mauchly_w.unwrap_or(f64::NAN), mauchly_p
            ));
        }
    }

    Ok(RepeatedAnovaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n_subjects * n_timepoints,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_subjects * n_timepoints, rows.len(), excluded),
        warnings,
        variable: value_col.to_string(),
        subject: subject_col.to_string(),
        time: time_col.to_string(),
        n_subjects,
        n_timepoints,
        time_f,
        time_df1: df_time,
        time_df2: df_error,
        time_p,
        mauchly_w,
        mauchly_p,
        gg_epsilon,
        gg_df1: gg_epsilon.map(|e| df_time as f64 * e),
        gg_df2: gg_epsilon.map(|e| df_error as f64 * e),
        gg_p,
        hf_epsilon,
        hf_df1: hf_epsilon.map(|e| df_time as f64 * e),
        hf_df2: hf_epsilon.map(|e| df_error as f64 * e),
        hf_p,
    })
}

/// Compute Mauchly's W sphericity test, Greenhouse-Geisser epsilon,
/// Huynh-Feldt epsilon, and corrected p-values.
fn mauchly_and_corrections(
    matrix: &[Vec<f64>],
    n_subjects: usize,
    n_timepoints: usize,
    time_f: f64,
    df_time: usize,
    df_error: usize,
) -> (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
) {
    if n_timepoints < 3 || n_subjects < 2 {
        return (None, None, None, None, None, None);
    }
    let p = n_timepoints;
    let n = n_subjects as f64;

    // Compute p x p sample covariance matrix
    let mut cov = vec![vec![0.0; p]; p];
    let mut means = vec![0.0; p];
    for row in matrix {
        for (j, &v) in row.iter().enumerate() {
            means[j] += v;
        }
    }
    for m in &mut means {
        *m /= n;
    }
    for j in 0..p {
        for k in 0..p {
            cov[j][k] = matrix
                .iter()
                .map(|row| (row[j] - means[j]) * (row[k] - means[k]))
                .sum::<f64>()
                / (n - 1.0);
        }
    }

    // Helmert contrasts: (p-1) x p
    let helmert = helmert_contrast_matrix(p);
    // cov_contrast = helmert * cov * helmert' = (p-1) x (p-1)
    let helmert_t: Vec<Vec<f64>> = (0..p)
        .map(|j| (0..(p - 1)).map(|i| helmert[i][j]).collect())
        .collect();
    let temp = matrix_multiply(&helmert, &cov).unwrap();
    let contrast_cov = matrix_multiply(&temp, &helmert_t).unwrap();

    let det_val = matrix_determinant(&contrast_cov);
    let trace_val = matrix_trace(&contrast_cov) / (p - 1) as f64;

    let mauchly_w = if trace_val > EPS {
        det_val / trace_val.powi(p as i32 - 1)
    } else {
        f64::NAN
    };

    // Box epsilon correction for chi-square
    let box_eps = 1.0 - (2.0 * (p - 1) as f64 * (p - 1) as f64 + 3.0 * (p - 1) as f64 - 1.0)
        / (6.0 * (p - 1) as f64 * (n - 1.0));

    let chi_sq = if mauchly_w > EPS && box_eps > 0.0 {
        -(n - 1.0) * box_eps * mauchly_w.ln()
    } else {
        f64::NAN
    };
    let mauchly_df = ((p - 1) * p / 2).saturating_sub(1);
    let mauchly_p = if chi_sq.is_finite() && mauchly_df > 0 {
        Some((1.0 - chi_square_cdf(chi_sq, mauchly_df as f64)).clamp(0.0, 1.0))
    } else {
        None
    };

    // Greenhouse-Geisser epsilon from eigenvalues of covariance matrix
    let (eigenvalues, _) = jacobi_eigh(cov);
    let sum_ev: f64 = eigenvalues.iter().sum();
    let sum_ev_sq: f64 = eigenvalues.iter().map(|&ev| ev * ev).sum();
    let gg_epsilon = if sum_ev_sq > EPS && p > 1 {
        Some((sum_ev * sum_ev / (p as f64 * sum_ev_sq)).clamp(1.0 / (p - 1) as f64, 1.0))
    } else {
        None
    };

    // Huynh-Feldt epsilon
    let hf_epsilon = gg_epsilon.map(|eps| {
        let n_eps = n * (p - 1) as f64 * eps;
        let denom = (p - 1) as f64 * (n - 1.0 - (p - 1) as f64 * eps);
        if denom > 0.0 {
            ((n_eps - 2.0) / denom).min(1.0)
        } else {
            1.0
        }
    });

    let gg_p = gg_epsilon.map(|eps| {
        let df1 = eps * df_time as f64;
        let df2 = eps * df_error as f64;
        f_distribution_p_value(time_f, df1.max(1.0), df2.max(1.0))
    });

    let hf_p = hf_epsilon.map(|eps| {
        let df1 = eps * df_time as f64;
        let df2 = eps * df_error as f64;
        f_distribution_p_value(time_f, df1.max(1.0), df2.max(1.0))
    });

    (
        Some(mauchly_w),
        mauchly_p,
        gg_epsilon,
        hf_epsilon,
        gg_p,
        hf_p,
    )
}
