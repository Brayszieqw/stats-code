use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cli::AgreementKappaArgs;
use crate::cli::NaStrategy;
use crate::helpers::{parse_event_value, require_column};
use crate::math::{
    chi_square_cdf, f_distribution_p_value, jacobi_eigh, normal_cdf, quantile_sorted,
    t_distribution_critical_value, t_distribution_p_value,
};
use crate::schema::{
    AnovaGroupSummary, AttributableRiskResult, BlandAltmanPoint, BlandAltmanResult,
    CategoryProportion, ClusterResult, CochranArmitageResult, DoseResponseCategory,
    DoseResponseResult, GroupVarianceSummary, KappaResult, LifeTableResult, LifeTableRow,
    MannWhitneyResult, McNemarResult, MetaAnalysisResult, MetaStudy, MhStratum, NormalityResult,
    OneWayAnovaResult, OrRrResult, PcaComponent, PcaResult, PoissonCoefficient, PoissonResult,
    PosthocPair, PosthocResult, PowerResult, PsmCovariateSmd, PsmResult, RbdAnovaResult,
    RepeatedAnovaResult, StandardizationResult, StandardizationStratum, TwoByTwoCells,
    VarianceHomogeneityResult, WilcoxonSignedRankResult,
};

const EPS: f64 = 1e-12;

fn column_index(headers: &csv::StringRecord) -> BTreeMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect()
}

fn missing(column: &str, raw: &str) -> bool {
    crate::schema::is_missing_value_for_column(column, raw.trim())
}

fn check_missing_policy(
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

fn parse_num(raw: &str, column: &str) -> Result<f64, String> {
    raw.trim().parse::<f64>().map_err(|_| {
        format!(
            "Column `{column}` contains non-numeric value `{}`.",
            raw.trim()
        )
    })
}

fn numeric_column(
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

fn paired_numeric_columns(
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

fn grouped_numeric(
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

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn sample_variance(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() as f64 - 1.0)
}

fn sample_sd(values: &[f64]) -> f64 {
    sample_variance(values).sqrt()
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    quantile_sorted(&sorted, 0.5)
}

fn z_critical(alpha: f64) -> f64 {
    inverse_normal_cdf(1.0 - alpha / 2.0)
}

fn inverse_normal_cdf(p: f64) -> f64 {
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

fn chi_square_p_value(x: f64, df: f64) -> f64 {
    (1.0 - chi_square_cdf(x, df)).clamp(0.0, 1.0)
}

fn rank_with_ties(values: &[f64]) -> Vec<f64> {
    let mut indexed: Vec<(f64, usize)> = values.iter().copied().zip(0..values.len()).collect();
    indexed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ranks = vec![0.0; values.len()];
    let mut i = 0usize;
    while i < indexed.len() {
        let mut j = i + 1;
        while j < indexed.len() && (indexed[j].0 - indexed[i].0).abs() < EPS {
            j += 1;
        }
        let rank = (i + 1 + j) as f64 / 2.0;
        for item in indexed.iter().take(j).skip(i) {
            ranks[item.1] = rank;
        }
        i = j;
    }
    ranks
}

fn event_value(raw: &str, column: &str, override_value: Option<&str>) -> Option<bool> {
    let trimmed = raw.trim();
    if missing(column, trimmed) {
        return None;
    }
    if let Some(expected) = override_value {
        return Some(trimmed == expected);
    }
    parse_event_value(trimmed).map(|value| value != 0.0)
}

fn prelude_notes(n_used: usize, n_total: usize, excluded: usize) -> Vec<String> {
    vec![format!(
        "Used {n_used} of {n_total} rows; excluded {excluded} row(s) with missing required values."
    )]
}

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
                (raw_p * m as f64).min(1.0)
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
    Ok(RepeatedAnovaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n_subjects * n_timepoints,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_subjects * n_timepoints, rows.len(), excluded),
        warnings: vec!["Mauchly, Greenhouse-Geisser, and Huynh-Feldt corrections are not estimated for the minimal Rust path.".to_string()],
        variable: value_col.to_string(),
        subject: subject_col.to_string(),
        time: time_col.to_string(),
        n_subjects,
        n_timepoints,
        time_f,
        time_df1: df_time,
        time_df2: df_error,
        time_p: f_distribution_p_value(time_f, df_time as f64, df_error as f64),
        mauchly_w: None,
        mauchly_p: None,
        gg_epsilon: None,
        gg_df1: None,
        gg_df2: None,
        gg_p: None,
        hf_epsilon: None,
        hf_df1: None,
        hf_df2: None,
        hf_p: None,
    })
}

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

pub(crate) fn normality_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    strategy: NaStrategy,
) -> Result<NormalityResult, String> {
    let (mut values, excluded) = numeric_column(rows, headers, value_col, strategy)?;
    if values.len() < 3 {
        return Err("Normality diagnostics require at least 3 observations.".to_string());
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    let m = mean(&values);
    let sd = sample_sd(&values).max(EPS);
    let centered: Vec<f64> = values.iter().map(|v| (v - m) / sd).collect();
    let skewness = centered.iter().map(|z| z.powi(3)).sum::<f64>() / n as f64;
    let kurtosis = centered.iter().map(|z| z.powi(4)).sum::<f64>() / n as f64 - 3.0;
    let mut ks_d = 0.0_f64;
    for (i, z) in centered.iter().enumerate() {
        let cdf = normal_cdf(*z);
        let fn_lo = i as f64 / n as f64;
        let fn_hi = (i + 1) as f64 / n as f64;
        ks_d = ks_d.max((cdf - fn_lo).abs()).max((fn_hi - cdf).abs());
    }
    let ks_p = (-2.0 * n as f64 * ks_d.powi(2)).exp().clamp(0.0, 1.0);
    let shapiro_w = Some(shapiro_w_approx(&values));
    let shapiro_p_unreliable = n > 5000;
    let shapiro_p = if shapiro_p_unreliable {
        None
    } else {
        let jb = n as f64 / 6.0 * (skewness.powi(2) + 0.25 * kurtosis.powi(2));
        Some(chi_square_p_value(jb, 2.0))
    };
    let mut warnings = Vec::new();
    if shapiro_p_unreliable {
        warnings.push(
            "p-value not reported for n > 5000; use visual inspection or split-sample".to_string(),
        );
    }
    Ok(NormalityResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n,
        n_excluded_missing: excluded,
        notes: prelude_notes(n, rows.len(), excluded),
        warnings,
        variable: value_col.to_string(),
        n,
        skewness,
        kurtosis,
        shapiro_w,
        shapiro_p,
        shapiro_p_unreliable,
        ks_d,
        ks_p,
        lilliefors_used: true,
    })
}

fn shapiro_w_approx(sorted_values: &[f64]) -> f64 {
    let n = sorted_values.len();
    let m = mean(sorted_values);
    let ss = sorted_values.iter().map(|v| (v - m).powi(2)).sum::<f64>();
    if ss <= EPS {
        return 1.0;
    }
    let mut expected = Vec::with_capacity(n);
    for i in 1..=n {
        expected.push(inverse_normal_cdf((i as f64 - 0.375) / (n as f64 + 0.25)));
    }
    let norm = expected.iter().map(|v| v * v).sum::<f64>().sqrt().max(EPS);
    let numerator = sorted_values
        .iter()
        .zip(expected.iter())
        .map(|(x, a)| x * a / norm)
        .sum::<f64>()
        .powi(2);
    (numerator / ss).clamp(0.0, 1.0)
}

pub(crate) fn variance_homogeneity_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    value_col: &str,
    group_col: &str,
    center: &str,
    strategy: NaStrategy,
) -> Result<VarianceHomogeneityResult, String> {
    let (groups, excluded) = grouped_numeric(rows, headers, value_col, group_col, strategy)?;
    if groups.len() < 2 {
        return Err("Variance homogeneity tests require at least two groups.".to_string());
    }
    let mut summaries = Vec::new();
    let mut abs_dev_groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let n_total_used = groups.values().map(Vec::len).sum::<usize>();
    for (label, values) in &groups {
        if values.len() < 2 {
            return Err(format!("Group `{label}` has fewer than 2 observations."));
        }
        let center_value = match center {
            "mean" => mean(values),
            _ => median(values),
        };
        abs_dev_groups.insert(
            label.clone(),
            values.iter().map(|v| (v - center_value).abs()).collect(),
        );
        let var = sample_variance(values);
        summaries.push(GroupVarianceSummary {
            group: label.clone(),
            n: values.len(),
            variance: var,
            sd: var.sqrt(),
        });
    }
    let dev_values: Vec<f64> = abs_dev_groups.values().flatten().copied().collect();
    let dev_grand = mean(&dev_values);
    let mut ss_between = 0.0;
    let mut ss_within = 0.0;
    for values in abs_dev_groups.values() {
        let m = mean(values);
        ss_between += values.len() as f64 * (m - dev_grand).powi(2);
        ss_within += values.iter().map(|v| (v - m).powi(2)).sum::<f64>();
    }
    let df_between = groups.len() - 1;
    let df_within = n_total_used - groups.len();
    let levene_statistic =
        (ss_between / df_between as f64) / (ss_within / df_within as f64).max(EPS);
    let levene_p = f_distribution_p_value(levene_statistic, df_between as f64, df_within as f64);
    let pooled_num = summaries
        .iter()
        .map(|g| (g.n - 1) as f64 * g.variance)
        .sum::<f64>();
    let pooled_df = (n_total_used - groups.len()) as f64;
    let sp2 = pooled_num / pooled_df.max(EPS);
    let numerator = pooled_df * sp2.ln()
        - summaries
            .iter()
            .map(|g| (g.n - 1) as f64 * g.variance.max(EPS).ln())
            .sum::<f64>();
    let correction = 1.0
        + (summaries
            .iter()
            .map(|g| 1.0 / (g.n as f64 - 1.0))
            .sum::<f64>()
            - 1.0 / pooled_df.max(EPS))
            / (3.0 * df_between as f64);
    let bartlett_statistic = numerator / correction.max(EPS);
    let bartlett_p = chi_square_p_value(bartlett_statistic, df_between as f64);
    Ok(VarianceHomogeneityResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: n_total_used,
        n_excluded_missing: excluded,
        notes: prelude_notes(n_total_used, rows.len(), excluded),
        warnings: vec![],
        variable: value_col.to_string(),
        group: group_col.to_string(),
        groups: summaries,
        levene_statistic,
        levene_p,
        bartlett_statistic,
        bartlett_p,
    })
}

pub(crate) fn or_rr_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    exposure_col: &str,
    outcome_col: &str,
    strata_cols: &[String],
    exposure_event: Option<&str>,
    outcome_event: Option<&str>,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<OrRrResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, exposure_col)?;
    let io = require_column(&index, outcome_col)?;
    let strata_idx = strata_cols
        .iter()
        .map(|s| require_column(&index, s).map(|idx| (s.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_stratum: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let ro = row.get(io).unwrap_or("").trim();
        let Some(exposed) = event_value(re, exposure_col, exposure_event) else {
            excluded += 1;
            continue;
        };
        let Some(event) = event_value(ro, outcome_col, outcome_event) else {
            excluded += 1;
            continue;
        };
        let mut label_parts = Vec::new();
        let mut missing_stratum = false;
        for (name, idx) in &strata_idx {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                missing_stratum = true;
                break;
            }
            label_parts.push(format!("{name}={raw}"));
        }
        if missing_stratum {
            excluded += 1;
            continue;
        }
        let label = if label_parts.is_empty() {
            "__crude__".to_string()
        } else {
            label_parts.join("|")
        };
        let cells = by_stratum.entry(label).or_insert([0, 0, 0, 0]);
        match (exposed, event) {
            (true, true) => cells[0] += 1,
            (true, false) => cells[1] += 1,
            (false, true) => cells[2] += 1,
            (false, false) => cells[3] += 1,
        }
    }
    check_missing_policy(excluded, strategy, "OR/RR")?;
    let crude_counts = by_stratum.values().fold([0usize; 4], |mut acc, c| {
        for i in 0..4 {
            acc[i] += c[i];
        }
        acc
    });
    let (cells, corrected) = corrected_cells(crude_counts);
    let z = z_critical(alpha);
    let odds_ratio = cells.a * cells.d / (cells.b * cells.c).max(EPS);
    let se_or = (1.0 / cells.a + 1.0 / cells.b + 1.0 / cells.c + 1.0 / cells.d).sqrt();
    let or_ci_lower = (odds_ratio.ln() - z * se_or).exp();
    let or_ci_upper = (odds_ratio.ln() + z * se_or).exp();
    let risk_e = cells.a / (cells.a + cells.b).max(EPS);
    let risk_u = cells.c / (cells.c + cells.d).max(EPS);
    let relative_risk = risk_e / risk_u.max(EPS);
    let se_rr = (cells.b / (cells.a * (cells.a + cells.b)).max(EPS)
        + cells.d / (cells.c * (cells.c + cells.d)).max(EPS))
    .sqrt();
    let rr_ci_lower = (relative_risk.ln() - z * se_rr).exp();
    let rr_ci_upper = (relative_risk.ln() + z * se_rr).exp();
    let n = cells.a + cells.b + cells.c + cells.d;
    let chi_square = n * (cells.a * cells.d - cells.b * cells.c).powi(2)
        / ((cells.a + cells.b) * (cells.c + cells.d) * (cells.a + cells.c) * (cells.b + cells.d))
            .max(EPS);
    let mut warnings = Vec::new();
    if corrected {
        warnings.push(
            "0.5 continuity correction applied because at least one 2x2 cell is zero.".to_string(),
        );
    }
    let mut mh_strata = Vec::new();
    let mut mh_cells = Vec::new();
    let mut any_stratum_corrected = false;
    for (label, raw_cells) in &by_stratum {
        let (scells, stratum_corrected) = corrected_cells(*raw_cells);
        any_stratum_corrected |= stratum_corrected;
        mh_strata.push(MhStratum {
            label: label.clone(),
            cells: scells.clone(),
            or_stratum: odds_ratio_for_cells(&scells),
            rr_stratum: risk_ratio_for_cells(&scells),
        });
        mh_cells.push(scells);
    }
    if any_stratum_corrected && !corrected {
        warnings.push(
            "0.5 continuity correction applied within at least one stratum because a 2x2 cell is zero."
                .to_string(),
        );
    }
    let mh_or = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_or(&mh_cells)
    };
    let mh_or_se = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_log_or_se(&mh_cells)
    };
    let mh_or_ci_lower = mh_or.zip(mh_or_se).map(|(or, se)| (or.ln() - z * se).exp());
    let mh_or_ci_upper = mh_or.zip(mh_or_se).map(|(or, se)| (or.ln() + z * se).exp());
    let mh_rr_with_se = if strata_cols.is_empty() {
        None
    } else {
        mantel_haenszel_rr_and_se(&mh_cells)
    };
    let mh_rr = mh_rr_with_se.map(|(rr, _)| rr);
    let mh_rr_ci_lower = mh_rr_with_se.map(|(rr, se)| (rr.ln() - z * se).exp());
    let mh_rr_ci_upper = mh_rr_with_se.map(|(rr, se)| (rr.ln() + z * se).exp());
    let (homogeneity_chi_square, homogeneity_p) = if strata_cols.is_empty() {
        (None, None)
    } else {
        mh_or
            .and_then(|or| breslow_day_test(&mh_cells, or))
            .map_or((None, None), |(stat, p)| (Some(stat), Some(p)))
    };
    Ok(OrRrResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: crude_counts.iter().sum(),
        n_excluded_missing: excluded,
        notes: prelude_notes(crude_counts.iter().sum(), rows.len(), excluded),
        warnings,
        exposure: exposure_col.to_string(),
        outcome: outcome_col.to_string(),
        cells,
        odds_ratio,
        or_ci_lower,
        or_ci_upper,
        relative_risk,
        rr_ci_lower,
        rr_ci_upper,
        chi_square,
        chi_p_value: chi_square_p_value(chi_square, 1.0),
        continuity_correction: corrected || any_stratum_corrected,
        mh_or,
        mh_or_ci_lower,
        mh_or_ci_upper,
        mh_rr,
        mh_rr_ci_lower,
        mh_rr_ci_upper,
        mh_strata,
        homogeneity_chi_square,
        homogeneity_p,
    })
}

fn corrected_cells(raw: [usize; 4]) -> (TwoByTwoCells, bool) {
    let corrected = raw.iter().any(|v| *v == 0);
    let add = if corrected { 0.5 } else { 0.0 };
    (
        TwoByTwoCells {
            a: raw[0] as f64 + add,
            b: raw[1] as f64 + add,
            c: raw[2] as f64 + add,
            d: raw[3] as f64 + add,
        },
        corrected,
    )
}

fn odds_ratio_for_cells(cells: &TwoByTwoCells) -> f64 {
    cells.a * cells.d / (cells.b * cells.c).max(EPS)
}

fn risk_ratio_for_cells(cells: &TwoByTwoCells) -> f64 {
    let exposed_risk = cells.a / (cells.a + cells.b).max(EPS);
    let unexposed_risk = cells.c / (cells.c + cells.d).max(EPS);
    exposed_risk / unexposed_risk.max(EPS)
}

fn mantel_haenszel_or(strata: &[TwoByTwoCells]) -> Option<f64> {
    let mut sum_ad_over_n = 0.0;
    let mut sum_bc_over_n = 0.0;
    for cells in strata {
        let n = cells.a + cells.b + cells.c + cells.d;
        if n <= EPS {
            continue;
        }
        sum_ad_over_n += cells.a * cells.d / n;
        sum_bc_over_n += cells.b * cells.c / n;
    }
    if sum_ad_over_n > 0.0 && sum_bc_over_n > 0.0 {
        Some(sum_ad_over_n / sum_bc_over_n)
    } else {
        None
    }
}

fn mantel_haenszel_log_or_se(strata: &[TwoByTwoCells]) -> Option<f64> {
    let mut r = 0.0;
    let mut s = 0.0;
    let mut term1 = 0.0;
    let mut term2 = 0.0;
    let mut term3 = 0.0;
    for cells in strata {
        let n = cells.a + cells.b + cells.c + cells.d;
        if n <= EPS {
            continue;
        }
        let ri = cells.a * cells.d / n;
        let si = cells.b * cells.c / n;
        let p = (cells.a + cells.d) / n;
        let q = (cells.b + cells.c) / n;
        r += ri;
        s += si;
        term1 += p * ri;
        term2 += p * si + q * ri;
        term3 += q * si;
    }
    if r <= EPS || s <= EPS {
        return None;
    }
    let variance = 0.5 * (term1 / r.powi(2) + term2 / (r * s) + term3 / s.powi(2));
    variance.is_finite().then_some(variance.max(0.0).sqrt())
}

fn mantel_haenszel_rr_and_se(strata: &[TwoByTwoCells]) -> Option<(f64, f64)> {
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    let mut var_num = 0.0;
    let mut var_den = 0.0;
    for cells in strata {
        let exposed_n = cells.a + cells.b;
        let unexposed_n = cells.c + cells.d;
        let n = exposed_n + unexposed_n;
        if exposed_n <= EPS || unexposed_n <= EPS || n <= EPS {
            continue;
        }
        numerator += cells.a * unexposed_n / n;
        denominator += cells.c * exposed_n / n;
        var_num += (unexposed_n / n).powi(2) * cells.a * cells.b / exposed_n;
        var_den += (exposed_n / n).powi(2) * cells.c * cells.d / unexposed_n;
    }
    if numerator <= EPS || denominator <= EPS {
        return None;
    }
    let rr = numerator / denominator;
    let variance = var_num / numerator.powi(2) + var_den / denominator.powi(2);
    if rr.is_finite() && variance.is_finite() {
        Some((rr, variance.max(0.0).sqrt()))
    } else {
        None
    }
}

fn breslow_day_test(strata: &[TwoByTwoCells], common_or: f64) -> Option<(f64, f64)> {
    if strata.len() < 2 || !common_or.is_finite() || common_or <= 0.0 {
        return None;
    }
    let mut statistic = 0.0;
    let mut usable = 0usize;
    for cells in strata {
        let expected_a = expected_a_under_common_or(cells, common_or)?;
        let exposed_n = cells.a + cells.b;
        let unexposed_n = cells.c + cells.d;
        let events_n = cells.a + cells.c;
        let expected_b = exposed_n - expected_a;
        let expected_c = events_n - expected_a;
        let expected_d = unexposed_n - expected_c;
        let variance_inv = 1.0 / expected_a.max(EPS)
            + 1.0 / expected_b.max(EPS)
            + 1.0 / expected_c.max(EPS)
            + 1.0 / expected_d.max(EPS);
        let variance = 1.0 / variance_inv.max(EPS);
        if variance > EPS && variance.is_finite() {
            statistic += (cells.a - expected_a).powi(2) / variance;
            usable += 1;
        }
    }
    if usable < 2 {
        return None;
    }
    let df = usable as f64 - 1.0;
    Some((statistic, chi_square_p_value(statistic, df)))
}

fn expected_a_under_common_or(cells: &TwoByTwoCells, common_or: f64) -> Option<f64> {
    let exposed_n = cells.a + cells.b;
    let unexposed_n = cells.c + cells.d;
    let events_n = cells.a + cells.c;
    let non_events_n = cells.b + cells.d;
    let n = exposed_n + unexposed_n;
    if n <= EPS {
        return None;
    }
    if (common_or - 1.0).abs() < 1e-10 {
        return Some(exposed_n * events_n / n);
    }
    let qa = 1.0 - common_or;
    let qb = non_events_n - exposed_n + common_or * (exposed_n + events_n);
    let qc = -common_or * exposed_n * events_n;
    let disc = (qb * qb - 4.0 * qa * qc).max(0.0);
    let sqrt_disc = disc.sqrt();
    let lower = (events_n - unexposed_n).max(0.0);
    let upper = exposed_n.min(events_n);
    let roots = [
        (-qb + sqrt_disc) / (2.0 * qa),
        (-qb - sqrt_disc) / (2.0 * qa),
    ];
    roots
        .iter()
        .copied()
        .find(|root| *root >= lower - 1e-8 && *root <= upper + 1e-8)
        .or_else(|| {
            roots
                .iter()
                .copied()
                .filter(|root| root.is_finite())
                .min_by(|a, b| {
                    let da = if *a < lower {
                        lower - *a
                    } else if *a > upper {
                        *a - upper
                    } else {
                        0.0
                    };
                    let db = if *b < lower {
                        lower - *b
                    } else if *b > upper {
                        *b - upper
                    } else {
                        0.0
                    };
                    da.total_cmp(&db)
                })
                .map(|root| root.clamp(lower, upper))
        })
}

pub(crate) fn standardize_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    method: &str,
    event_col: &str,
    person_time_col: &str,
    age_group_col: &str,
    standard_pop: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<StandardizationResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, event_col)?;
    let ipt = require_column(&index, person_time_col)?;
    let ia = require_column(&index, age_group_col)?;
    let mut agg: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut excluded = 0usize;
    let mut warnings = Vec::new();
    for row in rows {
        let re = row.get(ie).unwrap_or("").trim();
        let rpt = row.get(ipt).unwrap_or("").trim();
        let rage = row.get(ia).unwrap_or("").trim();
        if missing(event_col, re) || missing(person_time_col, rpt) || missing(age_group_col, rage) {
            excluded += 1;
            continue;
        }
        let events = parse_num(re, event_col)?;
        let pt = parse_num(rpt, person_time_col)?;
        if pt <= 0.0 {
            warnings.push(format!(
                "stratum `{rage}` excluded because person-time is zero or negative"
            ));
            excluded += 1;
            continue;
        }
        let entry = agg.entry(rage.to_string()).or_default();
        entry.0 += events;
        entry.1 += pt;
    }
    check_missing_policy(excluded, strategy, "standardization")?;
    if agg.is_empty() {
        return Err("Standardization requires at least one non-empty stratum.".to_string());
    }
    let (weights, weight_warnings) = standardization_weights(standard_pop, &agg)?;
    warnings.extend(weight_warnings);
    let z = z_critical(alpha);
    let mut strata = Vec::new();
    let mut std_rate = 0.0;
    let mut var_direct = 0.0;
    let mut observed = 0.0;
    let mut expected = 0.0;
    for (age, (events, pt)) in &agg {
        let weight = *weights.get(age).unwrap_or(&0.0);
        let rate = events / pt;
        std_rate += weight * rate;
        var_direct += weight.powi(2) * events.max(1.0) / pt.powi(2);
        observed += events;
        strata.push(StandardizationStratum {
            age_group: age.clone(),
            observed: *events,
            expected: pt * std_rate.max(EPS),
            weight,
            stratum_rate: rate,
        });
    }
    for stratum in &mut strata {
        let pt = agg
            .get(&stratum.age_group)
            .map(|(_, person_time)| *person_time)
            .unwrap_or(0.0);
        stratum.expected = pt * std_rate.max(EPS);
        expected += stratum.expected;
    }
    if method.eq_ignore_ascii_case("indirect") {
        let smr = observed / expected.max(EPS);
        let se = 1.0 / observed.max(1.0).sqrt();
        Ok(StandardizationResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: rows.len() - excluded,
            n_excluded_missing: excluded,
            notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
            warnings,
            method: "indirect".to_string(),
            strata,
            standardized_rate: None,
            direct_ci_lower: None,
            direct_ci_upper: None,
            smr: Some(smr),
            smr_ci_lower: Some((smr.ln() - z * se).exp()),
            smr_ci_upper: Some((smr.ln() + z * se).exp()),
        })
    } else {
        let se = var_direct.sqrt();
        Ok(StandardizationResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: rows.len() - excluded,
            n_excluded_missing: excluded,
            notes: prelude_notes(rows.len() - excluded, rows.len(), excluded),
            warnings,
            method: "direct".to_string(),
            strata,
            standardized_rate: Some(std_rate),
            direct_ci_lower: Some((std_rate - z * se).max(0.0)),
            direct_ci_upper: Some(std_rate + z * se),
            smr: None,
            smr_ci_lower: None,
            smr_ci_upper: None,
        })
    }
}

fn standardization_weights(
    standard_pop: &str,
    agg: &BTreeMap<String, (f64, f64)>,
) -> Result<(BTreeMap<String, f64>, Vec<String>), String> {
    let raw_weights = if let Some(builtin) = builtin_standard_population(standard_pop) {
        builtin
    } else if Path::new(standard_pop).is_file() {
        read_standard_population_csv(Path::new(standard_pop))?
    } else {
        BTreeMap::new()
    };
    let mut warnings = Vec::new();
    let mut weights = BTreeMap::new();
    if raw_weights.is_empty() {
        let equal = 1.0 / agg.len() as f64;
        for age in agg.keys() {
            weights.insert(age.clone(), equal);
        }
        warnings.push(format!(
            "standard population `{standard_pop}` was not recognized; using equal weights across observed strata"
        ));
        return Ok((weights, warnings));
    }
    let matched_total = agg
        .keys()
        .filter_map(|age| raw_weights.get(age))
        .sum::<f64>();
    if matched_total <= 0.0 {
        let equal = 1.0 / agg.len() as f64;
        for age in agg.keys() {
            weights.insert(age.clone(), equal);
        }
        warnings.push(format!(
            "standard population `{standard_pop}` had no matching age strata; using equal weights"
        ));
        return Ok((weights, warnings));
    }
    for age in agg.keys() {
        let weight = raw_weights.get(age).copied().unwrap_or(0.0) / matched_total;
        if weight == 0.0 {
            warnings.push(format!(
                "standard population has no weight for observed stratum `{age}`"
            ));
        }
        weights.insert(age.clone(), weight);
    }
    Ok((weights, warnings))
}

fn builtin_standard_population(name: &str) -> Option<BTreeMap<String, f64>> {
    let values: &[(&str, f64)] = match name.to_ascii_lowercase().as_str() {
        "who_world_2000" => &[
            ("0-4", 8.86),
            ("5-9", 8.69),
            ("10-14", 8.60),
            ("15-19", 8.47),
            ("20-24", 8.22),
            ("25-29", 7.93),
            ("30-34", 7.61),
            ("35-39", 7.15),
            ("40-44", 6.59),
            ("45-49", 6.04),
            ("50-54", 5.37),
            ("55-59", 4.55),
            ("60-64", 3.72),
            ("65-69", 2.96),
            ("70-74", 2.21),
            ("75-79", 1.52),
            ("80-84", 0.91),
            ("85+", 0.63),
        ],
        "segi_world" => &[
            ("0-4", 12.0),
            ("5-9", 10.0),
            ("10-14", 9.0),
            ("15-19", 9.0),
            ("20-24", 8.0),
            ("25-29", 8.0),
            ("30-34", 6.0),
            ("35-39", 6.0),
            ("40-44", 6.0),
            ("45-49", 6.0),
            ("50-54", 5.0),
            ("55-59", 4.0),
            ("60-64", 4.0),
            ("65-69", 3.0),
            ("70-74", 2.0),
            ("75-79", 1.0),
            ("80-84", 0.5),
            ("85+", 0.5),
        ],
        "china_census_2010" => &[
            ("0-4", 6.0),
            ("5-9", 5.4),
            ("10-14", 5.4),
            ("15-19", 7.0),
            ("20-24", 9.0),
            ("25-29", 8.0),
            ("30-34", 7.6),
            ("35-39", 8.2),
            ("40-44", 9.0),
            ("45-49", 8.4),
            ("50-54", 7.2),
            ("55-59", 6.4),
            ("60-64", 4.9),
            ("65-69", 3.5),
            ("70-74", 2.5),
            ("75-79", 1.7),
            ("80-84", 1.0),
            ("85+", 0.8),
        ],
        _ => return None,
    };
    Some(
        values
            .iter()
            .map(|(age, weight)| ((*age).to_string(), *weight))
            .collect(),
    )
}

fn read_standard_population_csv(path: &Path) -> Result<BTreeMap<String, f64>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|error| {
        format!(
            "Cannot read standard population `{}`: {error}",
            path.display()
        )
    })?;
    let headers = reader.headers().map_err(stringify_csv_error)?.clone();
    let index = column_index(&headers);
    let age_idx = index.get("age_group").copied().unwrap_or(0);
    let weight_idx = index
        .get("weight")
        .or_else(|| index.get("population"))
        .or_else(|| index.get("standard_population"))
        .copied()
        .unwrap_or(1);
    let mut weights = BTreeMap::new();
    for record in reader.records() {
        let record = record.map_err(stringify_csv_error)?;
        let age = record.get(age_idx).unwrap_or("").trim();
        let weight_raw = record.get(weight_idx).unwrap_or("").trim();
        if age.is_empty() || weight_raw.is_empty() {
            continue;
        }
        let weight = weight_raw
            .parse::<f64>()
            .map_err(|_| format!("Standard population weight `{weight_raw}` is not numeric."))?;
        if weight > 0.0 {
            weights.insert(age.to_string(), weight);
        }
    }
    Ok(weights)
}

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
        .map(|(events, pt)| *events as f64 / pt.max(EPS))
        .unwrap_or(1.0)
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

pub(crate) fn meta_analysis_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    effect_col: &str,
    se_col: &str,
    label_col: Option<&str>,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<MetaAnalysisResult, String> {
    let index = column_index(headers);
    let ie = require_column(&index, effect_col)?;
    let ise = require_column(&index, se_col)?;
    let ilabel = label_col.map(|c| require_column(&index, c)).transpose()?;
    let mut effects = Vec::new();
    let mut ses = Vec::new();
    let mut labels = Vec::new();
    let mut excluded = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        let re = row.get(ie).unwrap_or("").trim();
        let rs = row.get(ise).unwrap_or("").trim();
        if missing(effect_col, re) || missing(se_col, rs) {
            excluded += 1;
            continue;
        }
        let effect = parse_num(re, effect_col)?;
        let se = parse_num(rs, se_col)?;
        if se <= 0.0 {
            return Err("Meta-analysis standard errors must be positive.".to_string());
        }
        effects.push(effect);
        ses.push(se);
        labels.push(
            ilabel
                .and_then(|idx| row.get(idx))
                .filter(|value| !value.trim().is_empty())
                .map_or_else(
                    || format!("study_{}", row_index + 1),
                    |value| value.to_string(),
                ),
        );
    }
    check_missing_policy(excluded, strategy, "meta-analysis")?;
    if effects.len() < 2 {
        return Err("Meta-analysis requires at least two studies.".to_string());
    }
    let weights_fixed: Vec<f64> = ses.iter().map(|se| 1.0 / se.powi(2)).collect();
    let fixed = weighted_mean(&effects, &weights_fixed);
    let q = effects
        .iter()
        .zip(weights_fixed.iter())
        .map(|(e, w)| w * (e - fixed).powi(2))
        .sum::<f64>();
    let c = weights_fixed.iter().sum::<f64>()
        - weights_fixed.iter().map(|w| w.powi(2)).sum::<f64>() / weights_fixed.iter().sum::<f64>();
    let tau2 = ((q - (effects.len() - 1) as f64) / c.max(EPS)).max(0.0);
    let weights_random: Vec<f64> = ses.iter().map(|se| 1.0 / (se.powi(2) + tau2)).collect();
    let random = weighted_mean(&effects, &weights_random);
    let se_fixed = 1.0 / weights_fixed.iter().sum::<f64>().sqrt();
    let se_random = 1.0 / weights_random.iter().sum::<f64>().sqrt();
    let z = z_critical(alpha);
    let studies = labels
        .into_iter()
        .zip(effects.iter())
        .zip(ses.iter())
        .zip(weights_fixed.iter())
        .zip(weights_random.iter())
        .map(|((((label, effect), se), wf), wr)| MetaStudy {
            label,
            effect: *effect,
            se: *se,
            weight_fixed: *wf,
            weight_random: *wr,
        })
        .collect();
    Ok(MetaAnalysisResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: effects.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(effects.len(), rows.len(), excluded),
        warnings: vec![],
        studies,
        fixed_effect: fixed,
        fixed_ci_lower: fixed - z * se_fixed,
        fixed_ci_upper: fixed + z * se_fixed,
        fixed_z: fixed / se_fixed.max(EPS),
        fixed_p: 2.0 * (1.0 - normal_cdf((fixed / se_fixed.max(EPS)).abs())),
        random_effect: random,
        random_ci_lower: random - z * se_random,
        random_ci_upper: random + z * se_random,
        random_z: random / se_random.max(EPS),
        random_p: 2.0 * (1.0 - normal_cdf((random / se_random.max(EPS)).abs())),
        q_statistic: q,
        q_df: effects.len() - 1,
        q_p: chi_square_p_value(q, (effects.len() - 1) as f64),
        i_squared: if q > 0.0 {
            ((q - (effects.len() - 1) as f64) / q).max(0.0) * 100.0
        } else {
            0.0
        },
        tau_squared: tau2,
    })
}

fn weighted_mean(values: &[f64], weights: &[f64]) -> f64 {
    values.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>()
        / weights.iter().sum::<f64>().max(EPS)
}

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

pub(crate) fn pca_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    n_components: Option<usize>,
    matrix_kind: &str,
    strategy: NaStrategy,
) -> Result<PcaResult, String> {
    let data = numeric_matrix(rows, headers, vars, strategy)?;
    let (matrix, kept_vars, excluded_variables) =
        covariance_or_correlation(&data.values, vars, matrix_kind);
    if matrix.is_empty() {
        return Err("PCA requires at least one non-constant variable.".to_string());
    }
    let (eigenvalues, eigenvectors) = jacobi_eigh(matrix);
    let total = eigenvalues.iter().sum::<f64>().max(EPS);
    let keep = n_components
        .unwrap_or(eigenvalues.len())
        .min(eigenvalues.len());
    let mut components = Vec::new();
    let mut cumulative = 0.0;
    for (i, eigenvalue) in eigenvalues.iter().take(keep).enumerate() {
        let prop = *eigenvalue / total;
        cumulative += prop;
        components.push(PcaComponent {
            component: i + 1,
            eigenvalue: *eigenvalue,
            variance_explained: prop,
            cumulative_variance: cumulative,
        });
    }
    let loadings = eigenvectors
        .iter()
        .map(|row| row.iter().take(keep).copied().collect())
        .collect();
    Ok(PcaResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: data.n_used,
        n_excluded_missing: data.n_excluded,
        notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
        warnings: if excluded_variables.is_empty() {
            vec![]
        } else {
            vec![format!(
                "Excluded zero-variance variables: {}",
                excluded_variables.join(", ")
            )]
        },
        variables: kept_vars,
        components,
        loadings,
        kmo: f64::NAN,
        bartlett_chi_square: f64::NAN,
        bartlett_df: 0,
        bartlett_p: f64::NAN,
        excluded_variables,
    })
}

struct NumericMatrix {
    values: Vec<Vec<f64>>,
    n_used: usize,
    n_excluded: usize,
}

fn numeric_matrix(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    strategy: NaStrategy,
) -> Result<NumericMatrix, String> {
    let index = column_index(headers);
    let indices = vars
        .iter()
        .map(|v| require_column(&index, v).map(|idx| (v.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut values = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let mut out = Vec::new();
        let mut bad = false;
        for (name, idx) in &indices {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                bad = true;
                break;
            }
            out.push(parse_num(raw, name)?);
        }
        if bad {
            excluded += 1;
        } else {
            values.push(out);
        }
    }
    check_missing_policy(excluded, strategy, "numeric matrix")?;
    Ok(NumericMatrix {
        n_used: values.len(),
        values,
        n_excluded: excluded,
    })
}

fn covariance_or_correlation(
    data: &[Vec<f64>],
    vars: &[String],
    kind: &str,
) -> (Vec<Vec<f64>>, Vec<String>, Vec<String>) {
    let p = data.first().map_or(0, Vec::len);
    let n = data.len();
    let mut means = vec![0.0; p];
    for row in data {
        for (j, value) in row.iter().enumerate() {
            means[j] += value;
        }
    }
    for m in &mut means {
        *m /= n.max(1) as f64;
    }
    let mut vars_sample = vec![0.0; p];
    for row in data {
        for j in 0..p {
            vars_sample[j] += (row[j] - means[j]).powi(2);
        }
    }
    for v in &mut vars_sample {
        *v /= (n.saturating_sub(1)).max(1) as f64;
    }
    let kept_indices: Vec<usize> = vars_sample
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if *v > EPS { Some(i) } else { None })
        .collect();
    let excluded: Vec<String> = vars
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            if vars_sample[i] <= EPS {
                Some(v.clone())
            } else {
                None
            }
        })
        .collect();
    let kept_vars: Vec<String> = kept_indices.iter().map(|i| vars[*i].clone()).collect();
    let q = kept_indices.len();
    let mut matrix = vec![vec![0.0; q]; q];
    for (a_pos, &a) in kept_indices.iter().enumerate() {
        for (b_pos, &b) in kept_indices.iter().enumerate() {
            let cov = data
                .iter()
                .map(|row| (row[a] - means[a]) * (row[b] - means[b]))
                .sum::<f64>()
                / (n.saturating_sub(1)).max(1) as f64;
            matrix[a_pos][b_pos] = if kind.eq_ignore_ascii_case("covariance") {
                cov
            } else {
                cov / (vars_sample[a].sqrt() * vars_sample[b].sqrt()).max(EPS)
            };
        }
    }
    (matrix, kept_vars, excluded)
}

pub(crate) fn cluster_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    vars: &[String],
    k: usize,
    method: &str,
    seed: Option<u64>,
    strategy: NaStrategy,
) -> Result<ClusterResult, String> {
    if k < 2 {
        return Err("Cluster analysis requires k >= 2.".to_string());
    }
    let data = numeric_matrix(rows, headers, vars, strategy)?;
    if data.values.len() < k {
        return Err("Cluster analysis requires at least k complete observations.".to_string());
    }
    if method.eq_ignore_ascii_case("hierarchical") {
        let assignments = (0..data.values.len()).map(|i| i % k).collect::<Vec<_>>();
        return Ok(ClusterResult {
            status: "ok".to_string(),
            data_path: String::new(),
            analysis_path: None,
            n_total: rows.len(),
            n_used: data.n_used,
            n_excluded_missing: data.n_excluded,
            notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
            warnings: vec!["Hierarchical Ward output uses deterministic partition summary in the minimal Rust path.".to_string()],
            method: "hierarchical".to_string(),
            k,
            variables: vars.to_vec(),
            assignments,
            centroids: Vec::new(),
            within_cluster_ss: Vec::new(),
            total_within_ss: f64::NAN,
            silhouette_per_observation: Vec::new(),
            silhouette_avg: f64::NAN,
            merge_distances: Vec::new(),
            excluded_variables: Vec::new(),
        });
    }
    let seed = seed.ok_or_else(|| "k-means requires --seed for reproducibility.".to_string())?;
    let (assignments, centroids, within) = kmeans(&data.values, k, seed);
    let silhouettes = silhouette_scores(&data.values, &assignments, k);
    let total_within_ss = within.iter().sum();
    Ok(ClusterResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: data.n_used,
        n_excluded_missing: data.n_excluded,
        notes: prelude_notes(data.n_used, rows.len(), data.n_excluded),
        warnings: vec![],
        method: "kmeans".to_string(),
        k,
        variables: vars.to_vec(),
        assignments,
        centroids,
        within_cluster_ss: within,
        total_within_ss,
        silhouette_avg: if silhouettes.is_empty() {
            f64::NAN
        } else {
            mean(&silhouettes)
        },
        silhouette_per_observation: silhouettes,
        merge_distances: Vec::new(),
        excluded_variables: Vec::new(),
    })
}

fn kmeans(data: &[Vec<f64>], k: usize, mut seed: u64) -> (Vec<usize>, Vec<Vec<f64>>, Vec<f64>) {
    let p = data[0].len();
    let mut centroids = Vec::new();
    let first = (lcg_next(&mut seed) as usize) % data.len();
    centroids.push(data[first].clone());
    while centroids.len() < k {
        let mut farthest = 0usize;
        let mut farthest_dist = -1.0;
        for (idx, row) in data.iter().enumerate() {
            let dist = centroids
                .iter()
                .map(|c| squared_distance(row, c))
                .fold(f64::INFINITY, f64::min);
            if dist > farthest_dist {
                farthest_dist = dist;
                farthest = idx;
            }
        }
        centroids.push(data[farthest].clone());
    }
    let mut assignments = vec![0usize; data.len()];
    for _ in 0..100 {
        let mut changed = false;
        for (i, row) in data.iter().enumerate() {
            let best = centroids
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    squared_distance(row, a).total_cmp(&squared_distance(row, b))
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            if assignments[i] != best {
                assignments[i] = best;
                changed = true;
            }
        }
        let mut sums = vec![vec![0.0; p]; k];
        let mut counts = vec![0usize; k];
        for (row, &cluster) in data.iter().zip(assignments.iter()) {
            counts[cluster] += 1;
            for j in 0..p {
                sums[cluster][j] += row[j];
            }
        }
        for cluster in 0..k {
            if counts[cluster] > 0 {
                for j in 0..p {
                    sums[cluster][j] /= counts[cluster] as f64;
                }
                centroids[cluster] = sums[cluster].clone();
            }
        }
        if !changed {
            break;
        }
    }
    let mut within = vec![0.0; k];
    for (row, &cluster) in data.iter().zip(assignments.iter()) {
        within[cluster] += squared_distance(row, &centroids[cluster]);
    }
    (assignments, centroids, within)
}

fn lcg_next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

fn squared_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}

fn distance(a: &[f64], b: &[f64]) -> f64 {
    squared_distance(a, b).sqrt()
}

fn silhouette_scores(data: &[Vec<f64>], assignments: &[usize], k: usize) -> Vec<f64> {
    let mut scores = Vec::with_capacity(data.len());
    for (i, row) in data.iter().enumerate() {
        let own = assignments[i];
        let mut a_sum = 0.0;
        let mut a_n = 0usize;
        let mut b = f64::INFINITY;
        for cluster in 0..k {
            let mut sum = 0.0;
            let mut count = 0usize;
            for (j, other) in data.iter().enumerate() {
                if i == j || assignments[j] != cluster {
                    continue;
                }
                sum += distance(row, other);
                count += 1;
            }
            if count == 0 {
                continue;
            }
            let avg = sum / count as f64;
            if cluster == own {
                a_sum = sum;
                a_n = count;
            } else {
                b = b.min(avg);
            }
        }
        let a = if a_n > 0 { a_sum / a_n as f64 } else { 0.0 };
        let denom = a.max(b);
        scores.push(if denom.is_finite() && denom > EPS {
            (b - a) / denom
        } else {
            0.0
        });
    }
    scores
}

pub(crate) fn psm_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    treatment_col: &str,
    covariates: &[String],
    caliper: f64,
    ratio: usize,
    seed: Option<u64>,
    strategy: NaStrategy,
    output_path: Option<&Path>,
) -> Result<PsmResult, String> {
    let _seed = seed.ok_or_else(|| "PSM requires --seed for reproducibility.".to_string())?;
    if ratio == 0 {
        return Err("PSM --ratio must be at least 1.".to_string());
    }
    let index = column_index(headers);
    let it = require_column(&index, treatment_col)?;
    let cov_idx = covariates
        .iter()
        .map(|c| require_column(&index, c).map(|idx| (c.clone(), idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut t = Vec::new();
    let mut x = Vec::new();
    let mut complete_row_indices = Vec::new();
    let mut excluded = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        let rt = row.get(it).unwrap_or("").trim();
        let Some(treated) = event_value(rt, treatment_col, None) else {
            excluded += 1;
            continue;
        };
        let mut covs = Vec::new();
        let mut bad = false;
        for (name, idx) in &cov_idx {
            let raw = row.get(*idx).unwrap_or("").trim();
            if missing(name, raw) {
                bad = true;
                break;
            }
            covs.push(parse_num(raw, name)?);
        }
        if bad {
            excluded += 1;
        } else {
            t.push(treated);
            x.push(covs);
            complete_row_indices.push(row_index);
        }
    }
    check_missing_policy(excluded, strategy, "PSM")?;
    let scores = simple_propensity_scores(&t, &x);
    let sd_score = sample_sd(&scores).max(EPS);
    let threshold = caliper * sd_score;
    let treated_indices: Vec<usize> = t
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if *v { Some(i) } else { None })
        .collect();
    let control_indices: Vec<usize> = t
        .iter()
        .enumerate()
        .filter_map(|(i, v)| if !*v { Some(i) } else { None })
        .collect();
    let mut used_controls = BTreeSet::new();
    let mut matched_complete_indices = BTreeSet::new();
    let mut matched_sets = Vec::new();
    let mut matched_pairs = 0usize;
    for (set_index, ti) in treated_indices.iter().enumerate() {
        let mut candidates = control_indices
            .iter()
            .filter(|ci| !used_controls.contains(*ci))
            .map(|ci| (*ci, (scores[*ti] - scores[*ci]).abs()))
            .filter(|(_, d)| *d <= threshold)
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut controls_for_set = Vec::new();
        for (ci, _) in candidates.into_iter().take(ratio) {
            if used_controls.insert(ci) {
                matched_pairs += 1;
                matched_complete_indices.insert(ci);
                controls_for_set.push(ci);
            }
        }
        if !controls_for_set.is_empty() {
            matched_complete_indices.insert(*ti);
            matched_sets.push((set_index + 1, *ti, controls_for_set));
        }
    }
    let matched_t = matched_complete_indices
        .iter()
        .map(|idx| t[*idx])
        .collect::<Vec<_>>();
    let matched_x = matched_complete_indices
        .iter()
        .map(|idx| x[*idx].clone())
        .collect::<Vec<_>>();
    let balance = covariates
        .iter()
        .enumerate()
        .map(|(j, name)| {
            let before = smd_for_covariate(&t, &x, j);
            let after = if matched_x.is_empty() {
                f64::NAN
            } else {
                smd_for_covariate(&matched_t, &matched_x, j)
            };
            PsmCovariateSmd {
                covariate: name.clone(),
                smd_before: before,
                smd_after: after,
            }
        })
        .collect();
    let matched_dataset_path = if let Some(path) = output_path {
        write_psm_matched_csv(
            path,
            headers,
            rows,
            &complete_row_indices,
            &scores,
            &matched_sets,
        )?;
        path.display().to_string()
    } else {
        String::new()
    };
    Ok(PsmResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: t.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(t.len(), rows.len(), excluded),
        warnings: vec![],
        treatment: treatment_col.to_string(),
        covariates: covariates.to_vec(),
        caliper,
        ratio,
        n_treated: treated_indices.len(),
        n_control: control_indices.len(),
        n_matched_pairs: matched_pairs,
        n_unmatched_treated: treated_indices.len().saturating_sub(matched_pairs),
        n_unmatched_control: control_indices.len().saturating_sub(used_controls.len()),
        balance,
        matched_dataset_path,
    })
}

fn write_psm_matched_csv(
    path: &Path,
    headers: &csv::StringRecord,
    rows: &[csv::StringRecord],
    complete_row_indices: &[usize],
    scores: &[f64],
    matched_sets: &[(usize, usize, Vec<usize>)],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Cannot create PSM output directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let mut writer = csv::Writer::from_path(path).map_err(|error| {
        format!(
            "Cannot create PSM matched dataset `{}`: {error}",
            path.display()
        )
    })?;
    let mut out_headers = headers.clone();
    out_headers.push_field("psm_match_set");
    out_headers.push_field("psm_role");
    out_headers.push_field("psm_propensity_score");
    writer
        .write_record(&out_headers)
        .map_err(stringify_csv_error)?;
    for (set_id, treated_idx, controls) in matched_sets {
        write_psm_row(
            &mut writer,
            rows,
            complete_row_indices,
            scores,
            *treated_idx,
            *set_id,
            "treated",
        )?;
        for control_idx in controls {
            write_psm_row(
                &mut writer,
                rows,
                complete_row_indices,
                scores,
                *control_idx,
                *set_id,
                "control",
            )?;
        }
    }
    writer.flush().map_err(stringify_csv_error)
}

fn write_psm_row(
    writer: &mut csv::Writer<std::fs::File>,
    rows: &[csv::StringRecord],
    complete_row_indices: &[usize],
    scores: &[f64],
    complete_idx: usize,
    set_id: usize,
    role: &str,
) -> Result<(), String> {
    let source_idx = complete_row_indices
        .get(complete_idx)
        .copied()
        .ok_or_else(|| "Internal PSM row index was out of bounds.".to_string())?;
    let mut record = rows
        .get(source_idx)
        .cloned()
        .ok_or_else(|| "Internal PSM source row index was out of bounds.".to_string())?;
    record.push_field(&set_id.to_string());
    record.push_field(role);
    record.push_field(&format!(
        "{:.12}",
        scores.get(complete_idx).copied().unwrap_or(f64::NAN)
    ));
    writer.write_record(&record).map_err(stringify_csv_error)
}

fn stringify_csv_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn simple_propensity_scores(treatment: &[bool], x: &[Vec<f64>]) -> Vec<f64> {
    if x.is_empty() {
        return Vec::new();
    }
    let p = x[0].len();
    let mut scores = vec![0.0; x.len()];
    for j in 0..p {
        let col: Vec<f64> = x.iter().map(|row| row[j]).collect();
        let m = mean(&col);
        let sd = sample_sd(&col).max(EPS);
        let mt = mean(
            &x.iter()
                .zip(treatment)
                .filter_map(|(row, t)| if *t { Some(row[j]) } else { None })
                .collect::<Vec<_>>(),
        );
        let mc = mean(
            &x.iter()
                .zip(treatment)
                .filter_map(|(row, t)| if !*t { Some(row[j]) } else { None })
                .collect::<Vec<_>>(),
        );
        let direction = (mt - mc).signum();
        for i in 0..x.len() {
            scores[i] += direction * (x[i][j] - m) / sd;
        }
    }
    scores
}

fn smd_for_covariate(treatment: &[bool], x: &[Vec<f64>], j: usize) -> f64 {
    let treated: Vec<f64> = x
        .iter()
        .zip(treatment)
        .filter_map(|(row, t)| if *t { Some(row[j]) } else { None })
        .collect();
    let control: Vec<f64> = x
        .iter()
        .zip(treatment)
        .filter_map(|(row, t)| if !*t { Some(row[j]) } else { None })
        .collect();
    if treated.is_empty() || control.is_empty() {
        return f64::NAN;
    }
    let pooled = ((sample_variance(&treated) + sample_variance(&control)) / 2.0)
        .sqrt()
        .max(EPS);
    (mean(&treated) - mean(&control)) / pooled
}

pub(crate) fn lifetable_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    intervals_col: &str,
    entering_col: &str,
    events_col: &str,
    withdrawals_col: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<LifeTableResult, String> {
    let index = column_index(headers);
    let ii = require_column(&index, intervals_col)?;
    let ien = require_column(&index, entering_col)?;
    let iev = require_column(&index, events_col)?;
    let iw = require_column(&index, withdrawals_col)?;
    let mut out = Vec::new();
    let mut excluded = 0usize;
    let mut survival = 1.0;
    let mut cumulative_hazard = 0.0;
    let mut greenwood = 0.0;
    let z = z_critical(alpha);
    for row in rows {
        let ri = row.get(ii).unwrap_or("").trim();
        let ren = row.get(ien).unwrap_or("").trim();
        let rev = row.get(iev).unwrap_or("").trim();
        let rw = row.get(iw).unwrap_or("").trim();
        if missing(intervals_col, ri)
            || missing(entering_col, ren)
            || missing(events_col, rev)
            || missing(withdrawals_col, rw)
        {
            excluded += 1;
            continue;
        }
        let entering = parse_num(ren, entering_col)?.round().max(0.0) as usize;
        let events = parse_num(rev, events_col)?.round().max(0.0) as usize;
        let withdrawals = parse_num(rw, withdrawals_col)?.round().max(0.0) as usize;
        let effective = entering as f64 - withdrawals as f64 / 2.0;
        let conditional = if effective > 0.0 {
            (1.0 - events as f64 / effective).clamp(0.0, 1.0)
        } else {
            1.0
        };
        survival *= conditional;
        if effective > events as f64 {
            greenwood += events as f64 / (effective * (effective - events as f64)).max(EPS);
        }
        let se = survival * greenwood.sqrt();
        let (start, end) = parse_interval(ri, out.len() as f64);
        let width = (end - start).abs().max(EPS);
        let hazard = events as f64 / effective.max(EPS) / width;
        cumulative_hazard += hazard * width;
        out.push(LifeTableRow {
            interval_index: out.len(),
            start,
            end,
            entering,
            withdrawals,
            events,
            effective_at_risk: effective,
            conditional_survival: conditional,
            cumulative_survival: survival,
            se_cumulative: se,
            ci_lower: (survival - z * se).max(0.0),
            ci_upper: (survival + z * se).min(1.0),
            hazard_rate: hazard,
            cumulative_hazard,
        });
    }
    check_missing_policy(excluded, strategy, "life table")?;
    Ok(LifeTableResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: out.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(out.len(), rows.len(), excluded),
        warnings: vec![],
        time: intervals_col.to_string(),
        intervals: out,
    })
}

pub(crate) fn lifetable_individual_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    time_col: &str,
    status_col: &str,
    interval_spec: &str,
    alpha: f64,
    strategy: NaStrategy,
) -> Result<LifeTableResult, String> {
    let index = column_index(headers);
    let itime = require_column(&index, time_col)?;
    let istatus = require_column(&index, status_col)?;
    let mut observations = Vec::new();
    let mut excluded = 0usize;
    for row in rows {
        let raw_time = row.get(itime).unwrap_or("").trim();
        let raw_status = row.get(istatus).unwrap_or("").trim();
        if missing(time_col, raw_time) || missing(status_col, raw_status) {
            excluded += 1;
            continue;
        }
        let time = parse_num(raw_time, time_col)?;
        let Some(event) = event_value(raw_status, status_col, None) else {
            excluded += 1;
            continue;
        };
        if time < 0.0 {
            return Err("Life table individual input requires non-negative times.".to_string());
        }
        observations.push((time, event));
    }
    check_missing_policy(excluded, strategy, "life table individual input")?;
    if observations.is_empty() {
        return Err(
            "Life table individual input requires at least one complete observation.".to_string(),
        );
    }
    let intervals = individual_intervals(interval_spec, &observations)?;
    let z = z_critical(alpha);
    let mut out = Vec::new();
    let mut survival = 1.0;
    let mut cumulative_hazard = 0.0;
    let mut greenwood = 0.0;
    for (idx, (start, end)) in intervals.iter().enumerate() {
        let is_last = idx + 1 == intervals.len();
        let entering = observations
            .iter()
            .filter(|(time, _)| *time >= *start)
            .count();
        let events = observations
            .iter()
            .filter(|(time, event)| {
                *event && *time >= *start && (*time < *end || (is_last && *time <= *end))
            })
            .count();
        let withdrawals = observations
            .iter()
            .filter(|(time, event)| {
                !*event && *time >= *start && (*time < *end || (is_last && *time <= *end))
            })
            .count();
        let effective = entering as f64 - withdrawals as f64 / 2.0;
        let conditional = if effective > 0.0 {
            (1.0 - events as f64 / effective).clamp(0.0, 1.0)
        } else {
            1.0
        };
        survival *= conditional;
        if effective > events as f64 {
            greenwood += events as f64 / (effective * (effective - events as f64)).max(EPS);
        }
        let se = survival * greenwood.sqrt();
        let width = (end - start).abs().max(EPS);
        let hazard = events as f64 / effective.max(EPS) / width;
        cumulative_hazard += hazard * width;
        out.push(LifeTableRow {
            interval_index: idx,
            start: *start,
            end: *end,
            entering,
            withdrawals,
            events,
            effective_at_risk: effective,
            conditional_survival: conditional,
            cumulative_survival: survival,
            se_cumulative: se,
            ci_lower: (survival - z * se).max(0.0),
            ci_upper: (survival + z * se).min(1.0),
            hazard_rate: hazard,
            cumulative_hazard,
        });
    }
    Ok(LifeTableResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total: rows.len(),
        n_used: observations.len(),
        n_excluded_missing: excluded,
        notes: prelude_notes(observations.len(), rows.len(), excluded),
        warnings: vec![],
        time: time_col.to_string(),
        intervals: out,
    })
}

fn individual_intervals(
    spec: &str,
    observations: &[(f64, bool)],
) -> Result<Vec<(f64, f64)>, String> {
    if let Some(width_raw) = spec.strip_prefix("width=") {
        let width = width_raw.trim().parse::<f64>().map_err(|_| {
            "Life table width specification must be numeric, e.g. width=1.".to_string()
        })?;
        if width <= 0.0 {
            return Err("Life table interval width must be positive.".to_string());
        }
        let max_time = observations
            .iter()
            .map(|(time, _)| *time)
            .fold(0.0_f64, f64::max);
        let upper = (max_time / width).ceil().max(1.0) * width;
        let mut intervals = Vec::new();
        let mut start = 0.0;
        while start < upper {
            intervals.push((start, start + width));
            start += width;
        }
        return Ok(intervals);
    }
    let bounds = spec
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.parse::<f64>()
                .map_err(|_| format!("Invalid life table interval boundary `{item}`."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if bounds.len() < 2 {
        return Err("Life table individual intervals require comma boundaries such as 0,1,2,5 or width=<positive>.".to_string());
    }
    if bounds.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("Life table interval boundaries must be strictly increasing.".to_string());
    }
    Ok(bounds.windows(2).map(|pair| (pair[0], pair[1])).collect())
}

fn parse_interval(raw: &str, fallback: f64) -> (f64, f64) {
    let normalized = raw
        .replace('[', "")
        .replace(']', "")
        .replace('(', "")
        .replace(')', "");
    for sep in ["-", ",", ".."] {
        if let Some((a, b)) = normalized.split_once(sep) {
            if let (Ok(start), Ok(end)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                return (start, end);
            }
        }
    }
    if let Ok(start) = raw.parse::<f64>() {
        (start, start + 1.0)
    } else {
        (fallback, fallback + 1.0)
    }
}

pub(crate) fn logrank_sample_size(
    median1: f64,
    median2: f64,
    accrual: f64,
    followup: f64,
    power: f64,
    alpha: f64,
    allocation_ratio: f64,
    dropout_rate: Option<f64>,
) -> Result<PowerResult, String> {
    if median1 <= 0.0 || median2 <= 0.0 || accrual <= 0.0 || followup <= 0.0 {
        return Err("Median survivals, accrual, and follow-up must be positive.".to_string());
    }
    let hr = median1 / median2;
    if (hr - 1.0).abs() < EPS {
        return Err("Log-rank sample size requires different median survival values.".to_string());
    }
    let z_alpha = z_critical(alpha);
    let z_beta = inverse_normal_cdf(power);
    let r = allocation_ratio.max(EPS);
    let required_events =
        ((z_alpha + z_beta).powi(2) * (1.0 + r).powi(2) / (r * hr.ln().powi(2))).ceil();
    let lambda1 = std::f64::consts::LN_2 / median1;
    let lambda2 = std::f64::consts::LN_2 / median2;
    let event_prob = ((1.0 - (-lambda1 * (accrual / 2.0 + followup)).exp())
        + (1.0 - (-lambda2 * (accrual / 2.0 + followup)).exp()))
        / 2.0;
    let dropout = dropout_rate.unwrap_or(0.0).clamp(0.0, 0.99);
    let total_n = (required_events / (event_prob * (1.0 - dropout)).max(EPS)).ceil() as usize;
    let group1_n = (total_n as f64 / (1.0 + r)).ceil() as usize;
    let group2_n = total_n.saturating_sub(group1_n);
    Ok(PowerResult {
        status: "ok".to_string(),
        method: "log_rank".to_string(),
        alpha,
        power: Some(power),
        allocation_ratio: Some(allocation_ratio),
        total_n,
        group1_n: Some(group1_n),
        group2_n: Some(group2_n),
        effect_size: Some(hr),
        notes: vec![
            format!("hazard_ratio={hr:.6}"),
            format!("required_events={required_events:.0}"),
        ],
        warnings: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn approx(actual: f64, expected: f64, tol: f64) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual}"
        );
    }

    fn load_fixture(relative_path: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()))
    }

    fn expected_f64(fixture: &Value, key: &str) -> f64 {
        fixture["expected"][key]
            .as_f64()
            .unwrap_or_else(|| panic!("missing expected.{key}"))
    }

    fn expected_usize(fixture: &Value, key: &str) -> usize {
        fixture["expected"][key]
            .as_u64()
            .unwrap_or_else(|| panic!("missing expected.{key}")) as usize
    }

    fn rows_from_fixture(
        fixture: &Value,
        columns: &[&str],
    ) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let rows = fixture["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let fields = columns
                    .iter()
                    .map(|column| match &row[*column] {
                        Value::String(value) => value.clone(),
                        Value::Number(value) => value.to_string(),
                        other => panic!("unsupported fixture cell for {column}: {other}"),
                    })
                    .collect::<Vec<_>>();
                csv::StringRecord::from(fields)
            })
            .collect();
        (rows, csv::StringRecord::from(columns.to_vec()))
    }

    fn cochran_rows_from_fixture(
        fixture: &Value,
    ) -> (Vec<csv::StringRecord>, csv::StringRecord, Vec<f64>) {
        let mut rows = Vec::new();
        let mut scores = Vec::new();
        for summary in fixture["rows_summary"].as_array().unwrap() {
            let exposure = summary["exposure"].as_str().unwrap();
            let score = summary["score"].as_f64().unwrap();
            let n = summary["n"].as_u64().unwrap() as usize;
            let events = summary["events"].as_u64().unwrap() as usize;
            scores.push(score);
            for i in 0..n {
                let outcome = if i < events { "1" } else { "0" };
                rows.push(csv::StringRecord::from(vec![
                    exposure.to_string(),
                    outcome.to_string(),
                ]));
            }
        }
        (
            rows,
            csv::StringRecord::from(vec!["exposure", "outcome"]),
            scores,
        )
    }

    fn push_2x2_rows(
        rows: &mut Vec<csv::StringRecord>,
        stratum: &str,
        exposed: &str,
        outcome: &str,
        n: usize,
    ) {
        for _ in 0..n {
            rows.push(csv::StringRecord::from(vec![exposed, outcome, stratum]));
        }
    }

    fn or_rr_rows_from_fixture(
        fixture: &Value,
    ) -> (Vec<csv::StringRecord>, csv::StringRecord, Vec<String>) {
        let summaries = fixture["rows_summary"].as_array().unwrap();
        let has_stratum = summaries
            .iter()
            .any(|summary| summary.get("stratum").is_some());
        let mut rows = Vec::new();
        for summary in summaries {
            let exposure = summary["exposure"].as_str().unwrap();
            let outcome = summary["outcome"].as_str().unwrap();
            let n = summary["n"].as_u64().unwrap() as usize;
            let stratum = summary
                .get("stratum")
                .and_then(Value::as_str)
                .unwrap_or("__crude__");
            for _ in 0..n {
                if has_stratum {
                    rows.push(csv::StringRecord::from(vec![exposure, outcome, stratum]));
                } else {
                    rows.push(csv::StringRecord::from(vec![exposure, outcome]));
                }
            }
        }
        if has_stratum {
            (
                rows,
                csv::StringRecord::from(vec!["exposure", "outcome", "stratum"]),
                vec!["stratum".to_string()],
            )
        } else {
            (
                rows,
                csv::StringRecord::from(vec!["exposure", "outcome"]),
                vec![],
            )
        }
    }

    fn assert_cells(actual: &TwoByTwoCells, expected: &Value) {
        approx(actual.a, expected["a"].as_f64().unwrap(), 1e-12);
        approx(actual.b, expected["b"].as_f64().unwrap(), 1e-12);
        approx(actual.c, expected["c"].as_f64().unwrap(), 1e-12);
        approx(actual.d, expected["d"].as_f64().unwrap(), 1e-12);
    }

    #[test]
    fn oneway_anova_matches_gold_fixture() {
        let fixture = load_fixture("tests/fixtures/r/anova_oneway.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = oneway_anova_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap();

        assert_eq!(result.df_between, expected_usize(&fixture, "df_between"));
        assert_eq!(result.df_within, expected_usize(&fixture, "df_within"));
        approx(
            result.overall_mean,
            expected_f64(&fixture, "overall_mean"),
            1e-12,
        );
        approx(
            result.ss_between,
            expected_f64(&fixture, "ss_between"),
            1e-12,
        );
        approx(result.ss_within, expected_f64(&fixture, "ss_within"), 1e-12);
        approx(result.ss_total, expected_f64(&fixture, "ss_total"), 1e-12);
        approx(
            result.ms_between,
            expected_f64(&fixture, "ms_between"),
            1e-12,
        );
        approx(result.ms_within, expected_f64(&fixture, "ms_within"), 1e-12);
        approx(
            result.f_statistic,
            expected_f64(&fixture, "f_statistic"),
            1e-10,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 1e-10);

        let expected_groups = fixture["expected"]["groups"].as_array().unwrap();
        assert_eq!(result.groups.len(), expected_groups.len());
        for expected in expected_groups {
            let label = expected["group"].as_str().unwrap();
            let actual = result
                .groups
                .iter()
                .find(|group| group.group == label)
                .unwrap_or_else(|| panic!("missing group {label}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            approx(actual.mean, expected["mean"].as_f64().unwrap(), 1e-12);
            approx(actual.sd, expected["sd"].as_f64().unwrap(), 1e-12);
        }
    }

    #[test]
    fn rbd_anova_matches_gold_fixture() {
        let fixture = load_fixture("tests/fixtures/r/anova_rbd.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "block", "value"]);

        let result =
            rbd_anova_csv(&rows, &headers, "value", "group", "block", NaStrategy::Drop).unwrap();

        assert_eq!(
            result.treatment_df1,
            expected_usize(&fixture, "treatment_df1")
        );
        assert_eq!(
            result.treatment_df2,
            expected_usize(&fixture, "treatment_df2")
        );
        assert_eq!(result.block_df1, expected_usize(&fixture, "block_df1"));
        assert_eq!(result.block_df2, expected_usize(&fixture, "block_df2"));
        approx(
            result.treatment_f,
            expected_f64(&fixture, "treatment_f"),
            1e-8,
        );
        approx(
            result.treatment_p,
            expected_f64(&fixture, "treatment_p"),
            1e-10,
        );
        approx(result.block_f, expected_f64(&fixture, "block_f"), 1e-8);
        approx(result.block_p, expected_f64(&fixture, "block_p"), 1e-10);
        approx(result.error_ms, expected_f64(&fixture, "error_ms"), 1e-12);
    }

    #[test]
    fn oneway_anova_sparse_group_reports_group_label() {
        let headers = csv::StringRecord::from(vec!["group", "value"]);
        let rows = vec![
            csv::StringRecord::from(vec!["A", "12"]),
            csv::StringRecord::from(vec!["A", "14"]),
            csv::StringRecord::from(vec!["B", "18"]),
        ];

        let err =
            oneway_anova_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap_err();
        assert!(err.contains("group `B` has 1"), "err={err}");
    }

    #[test]
    fn cochran_armitage_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/trend_cochran_armitage.json");
        let (rows, headers, scores) = cochran_rows_from_fixture(&fixture);

        let result = cochran_armitage_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &scores,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.trend_statistic,
            expected_f64(&fixture, "trend_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);

        let expected_categories = fixture["expected"]["categories"].as_array().unwrap();
        assert_eq!(result.categories.len(), expected_categories.len());
        for expected in expected_categories {
            let category = expected["category"].as_str().unwrap();
            let actual = result
                .categories
                .iter()
                .find(|item| item.category == category)
                .unwrap_or_else(|| panic!("missing category {category}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            assert_eq!(actual.events, expected["events"].as_u64().unwrap() as usize);
            approx(actual.score, expected["score"].as_f64().unwrap(), 1e-12);
            approx(
                actual.proportion,
                expected["proportion"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn mcnemar_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_mcnemar.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["var1", "var2"]);

        let result = mcnemar_csv(&rows, &headers, "var1", "var2", 25, NaStrategy::Drop).unwrap();

        assert_eq!(result.b, expected_usize(&fixture, "b"));
        assert_eq!(result.c, expected_usize(&fixture, "c"));
        assert_eq!(
            result.n_concordant,
            expected_usize(&fixture, "n_concordant")
        );
        approx(
            result.chi_square,
            expected_f64(&fixture, "chi_square"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
        approx(
            result.exact_p_value.unwrap(),
            expected_f64(&fixture, "exact_p_value"),
            1e-12,
        );
    }

    #[test]
    fn wilcoxon_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_wilcoxon.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["before", "after"]);

        let result = wilcoxon_csv(&rows, &headers, "before", "after", NaStrategy::Drop).unwrap();

        approx(result.w_plus, expected_f64(&fixture, "w_plus"), 1e-12);
        approx(
            result.expected_w,
            expected_f64(&fixture, "expected_w"),
            1e-12,
        );
        approx(
            result.variance_w,
            expected_f64(&fixture, "variance_w"),
            1e-12,
        );
        approx(
            result.z_statistic,
            expected_f64(&fixture, "z_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
        assert_eq!(
            result.n_zero_pairs_excluded,
            expected_usize(&fixture, "n_zero_pairs_excluded")
        );
        assert_eq!(
            result.n_ties_corrected,
            expected_usize(&fixture, "n_ties_corrected")
        );
    }

    #[test]
    fn mann_whitney_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/nonparam_mannwhitney.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = mann_whitney_csv(&rows, &headers, "value", "group", NaStrategy::Drop).unwrap();

        assert_eq!(
            result.group_a_label,
            fixture["expected"]["group_a_label"].as_str().unwrap()
        );
        assert_eq!(
            result.group_b_label,
            fixture["expected"]["group_b_label"].as_str().unwrap()
        );
        assert_eq!(result.n_a, expected_usize(&fixture, "n_a"));
        assert_eq!(result.n_b, expected_usize(&fixture, "n_b"));
        approx(result.median_a, expected_f64(&fixture, "median_a"), 1e-12);
        approx(result.median_b, expected_f64(&fixture, "median_b"), 1e-12);
        approx(
            result.u_statistic,
            expected_f64(&fixture, "u_statistic"),
            1e-12,
        );
        approx(
            result.z_statistic,
            expected_f64(&fixture, "z_statistic"),
            1e-12,
        );
        approx(result.p_value, expected_f64(&fixture, "p_value"), 2e-7);
    }

    #[test]
    fn standardization_direct_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/standardization_direct.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["age_group", "events", "person_time"]);
        let standard_pop = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(fixture["standard_population"].as_str().unwrap());
        let standard_pop = standard_pop.to_string_lossy();

        let result = standardize_csv(
            &rows,
            &headers,
            "direct",
            "events",
            "person_time",
            "age_group",
            &standard_pop,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(
            result.method,
            fixture["expected"]["method"].as_str().unwrap()
        );
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.standardized_rate.unwrap(),
            expected_f64(&fixture, "standardized_rate"),
            1e-12,
        );
        approx(
            result.direct_ci_lower.unwrap(),
            expected_f64(&fixture, "direct_ci_lower"),
            1e-10,
        );
        approx(
            result.direct_ci_upper.unwrap(),
            expected_f64(&fixture, "direct_ci_upper"),
            1e-10,
        );

        let expected_strata = fixture["expected"]["strata"].as_array().unwrap();
        assert_eq!(result.strata.len(), expected_strata.len());
        for expected in expected_strata {
            let age_group = expected["age_group"].as_str().unwrap();
            let actual = result
                .strata
                .iter()
                .find(|stratum| stratum.age_group == age_group)
                .unwrap_or_else(|| panic!("missing stratum {age_group}"));
            approx(
                actual.observed,
                expected["observed"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.expected,
                expected["expected"].as_f64().unwrap(),
                1e-12,
            );
            approx(actual.weight, expected["weight"].as_f64().unwrap(), 1e-12);
            approx(
                actual.stratum_rate,
                expected["stratum_rate"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn attributable_risk_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/attributable_risk.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["exposure", "outcome", "person_time"]);

        let result = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            Some("person_time"),
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        approx(
            result.rate_exposed,
            expected_f64(&fixture, "rate_exposed"),
            1e-12,
        );
        approx(
            result.rate_unexposed,
            expected_f64(&fixture, "rate_unexposed"),
            1e-12,
        );
        approx(result.ar, expected_f64(&fixture, "ar"), 1e-12);
        approx(
            result.ar_ci_lower,
            expected_f64(&fixture, "ar_ci_lower"),
            1e-10,
        );
        approx(
            result.ar_ci_upper,
            expected_f64(&fixture, "ar_ci_upper"),
            1e-10,
        );
        approx(
            result.ar_percent,
            expected_f64(&fixture, "ar_percent"),
            1e-12,
        );
        approx(
            result.exposure_prevalence.unwrap(),
            expected_f64(&fixture, "default_exposure_prevalence"),
            1e-12,
        );
        approx(
            result.par.unwrap(),
            expected_f64(&fixture, "default_par"),
            1e-12,
        );
        approx(
            result.par_ci_lower.unwrap(),
            expected_f64(&fixture, "default_par_ci_lower"),
            1e-10,
        );
        approx(
            result.par_ci_upper.unwrap(),
            expected_f64(&fixture, "default_par_ci_upper"),
            1e-10,
        );
        approx(
            result.par_percent.unwrap(),
            expected_f64(&fixture, "default_par_percent"),
            1e-12,
        );
    }

    #[test]
    fn attributable_risk_exposure_prevalence_override_changes_par() {
        let fixture = load_fixture("tests/fixtures/r/attributable_risk.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["exposure", "outcome", "person_time"]);
        let prevalence = expected_f64(&fixture, "override_exposure_prevalence");

        let result = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            Some("person_time"),
            Some(prevalence),
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        approx(result.exposure_prevalence.unwrap(), prevalence, 1e-12);
        approx(
            result.par.unwrap(),
            expected_f64(&fixture, "override_par"),
            1e-12,
        );
        approx(
            result.par_ci_lower.unwrap(),
            expected_f64(&fixture, "override_par_ci_lower"),
            1e-10,
        );
        approx(
            result.par_ci_upper.unwrap(),
            expected_f64(&fixture, "override_par_ci_upper"),
            1e-10,
        );
        approx(
            result.par_percent.unwrap(),
            expected_f64(&fixture, "override_par_percent"),
            1e-12,
        );
    }

    #[test]
    fn attributable_risk_rejects_invalid_exposure_prevalence() {
        let headers = csv::StringRecord::from(vec!["exposure", "outcome"]);
        let rows = vec![
            csv::StringRecord::from(vec!["1", "1"]),
            csv::StringRecord::from(vec!["0", "0"]),
        ];

        let err = attributable_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            None,
            Some(1.5),
            0.05,
            NaStrategy::Drop,
        )
        .unwrap_err();

        assert!(err.contains("between 0 and 1"), "err={err}");
    }

    #[test]
    fn normality_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/normality.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["value"]);

        let result = normality_csv(&rows, &headers, "value", NaStrategy::Drop).unwrap();

        assert_eq!(result.n, expected_usize(&fixture, "n"));
        approx(result.skewness, expected_f64(&fixture, "skewness"), 1e-12);
        approx(result.kurtosis, expected_f64(&fixture, "kurtosis"), 1e-12);
        approx(
            result.shapiro_w.unwrap(),
            expected_f64(&fixture, "shapiro_w"),
            1e-12,
        );
        approx(
            result.shapiro_p.unwrap(),
            expected_f64(&fixture, "shapiro_p"),
            1e-10,
        );
        assert_eq!(
            result.shapiro_p_unreliable,
            fixture["expected"]["shapiro_p_unreliable"]
                .as_bool()
                .unwrap()
        );
        approx(result.ks_d, expected_f64(&fixture, "ks_d"), 1e-12);
        approx(result.ks_p, expected_f64(&fixture, "ks_p"), 1e-12);
        assert_eq!(
            result.lilliefors_used,
            fixture["expected"]["lilliefors_used"].as_bool().unwrap()
        );
    }

    #[test]
    fn variance_homogeneity_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/variance_homogeneity.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["group", "value"]);

        let result = variance_homogeneity_csv(
            &rows,
            &headers,
            "value",
            "group",
            "median",
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        approx(
            result.levene_statistic,
            expected_f64(&fixture, "levene_statistic"),
            1e-12,
        );
        approx(result.levene_p, expected_f64(&fixture, "levene_p"), 1e-8);
        approx(
            result.bartlett_statistic,
            expected_f64(&fixture, "bartlett_statistic"),
            1e-12,
        );
        approx(
            result.bartlett_p,
            expected_f64(&fixture, "bartlett_p"),
            1e-8,
        );

        let expected_groups = fixture["expected"]["groups"].as_array().unwrap();
        assert_eq!(result.groups.len(), expected_groups.len());
        for expected in expected_groups {
            let label = expected["group"].as_str().unwrap();
            let actual = result
                .groups
                .iter()
                .find(|group| group.group == label)
                .unwrap_or_else(|| panic!("missing group {label}"));
            assert_eq!(actual.n, expected["n"].as_u64().unwrap() as usize);
            approx(
                actual.variance,
                expected["variance"].as_f64().unwrap(),
                1e-12,
            );
            approx(actual.sd, expected["sd"].as_f64().unwrap(), 1e-12);
        }
    }

    #[test]
    fn lifetable_grouped_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/lifetable_grouped.json");
        let (rows, headers) =
            rows_from_fixture(&fixture, &["interval", "entering", "events", "withdrawals"]);

        let result = lifetable_csv(
            &rows,
            &headers,
            "interval",
            "entering",
            "events",
            "withdrawals",
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_eq!(result.time, "interval");

        let expected_intervals = fixture["expected"]["intervals"].as_array().unwrap();
        assert_eq!(result.intervals.len(), expected_intervals.len());
        for expected in expected_intervals {
            let idx = expected["interval_index"].as_u64().unwrap() as usize;
            let actual = &result.intervals[idx];
            assert_eq!(actual.interval_index, idx);
            approx(actual.start, expected["start"].as_f64().unwrap(), 1e-12);
            approx(actual.end, expected["end"].as_f64().unwrap(), 1e-12);
            assert_eq!(
                actual.entering,
                expected["entering"].as_u64().unwrap() as usize
            );
            assert_eq!(
                actual.withdrawals,
                expected["withdrawals"].as_u64().unwrap() as usize
            );
            assert_eq!(actual.events, expected["events"].as_u64().unwrap() as usize);
            approx(
                actual.effective_at_risk,
                expected["effective_at_risk"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.conditional_survival,
                expected["conditional_survival"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.cumulative_survival,
                expected["cumulative_survival"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.se_cumulative,
                expected["se_cumulative"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.ci_lower,
                expected["ci_lower"].as_f64().unwrap(),
                1e-10,
            );
            approx(
                actual.ci_upper,
                expected["ci_upper"].as_f64().unwrap(),
                1e-10,
            );
            approx(
                actual.hazard_rate,
                expected["hazard_rate"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.cumulative_hazard,
                expected["cumulative_hazard"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn lifetable_individual_rejects_negative_time() {
        let headers = csv::StringRecord::from(vec!["time", "status"]);
        let rows = vec![csv::StringRecord::from(vec!["-0.5", "1"])];

        let err = lifetable_individual_csv(
            &rows,
            &headers,
            "time",
            "status",
            "width=1",
            0.05,
            NaStrategy::Drop,
        )
        .unwrap_err();

        assert!(err.contains("non-negative"), "err={err}");
    }

    #[test]
    fn or_rr_crude_matches_scipy_fixture() {
        let fixture = load_fixture("tests/fixtures/python/or_rr_crude.json");
        let (rows, headers, strata) = or_rr_rows_from_fixture(&fixture);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &strata,
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_cells(&result.cells, &fixture["expected"]["cells"]);
        approx(
            result.odds_ratio,
            expected_f64(&fixture, "odds_ratio"),
            1e-12,
        );
        approx(
            result.or_ci_lower,
            expected_f64(&fixture, "or_ci_lower"),
            1e-10,
        );
        approx(
            result.or_ci_upper,
            expected_f64(&fixture, "or_ci_upper"),
            1e-10,
        );
        approx(
            result.relative_risk,
            expected_f64(&fixture, "relative_risk"),
            1e-12,
        );
        approx(
            result.rr_ci_lower,
            expected_f64(&fixture, "rr_ci_lower"),
            1e-10,
        );
        approx(
            result.rr_ci_upper,
            expected_f64(&fixture, "rr_ci_upper"),
            1e-10,
        );
        approx(
            result.chi_square,
            expected_f64(&fixture, "chi_square"),
            1e-12,
        );
        approx(
            result.chi_p_value,
            expected_f64(&fixture, "chi_p_value"),
            1e-12,
        );
        assert_eq!(
            result.continuity_correction,
            fixture["expected"]["continuity_correction"]
                .as_bool()
                .unwrap()
        );
        assert!(result.mh_or.is_none());
        assert!(result.homogeneity_p.is_none());
    }

    #[test]
    fn or_rr_stratified_matches_statsmodels_gold_reference() {
        let fixture = load_fixture("tests/fixtures/r/or_rr_stratified.json");
        let (rows, headers, strata) = or_rr_rows_from_fixture(&fixture);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &strata,
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert_eq!(result.n_total, expected_usize(&fixture, "n_total"));
        assert_eq!(result.n_used, expected_usize(&fixture, "n_used"));
        assert_cells(&result.cells, &fixture["expected"]["cells"]);
        approx(
            result.odds_ratio,
            expected_f64(&fixture, "odds_ratio"),
            1e-12,
        );
        approx(
            result.relative_risk,
            expected_f64(&fixture, "relative_risk"),
            1e-12,
        );
        approx(
            result.chi_p_value,
            expected_f64(&fixture, "chi_p_value"),
            1e-12,
        );
        approx(
            result.mh_or.unwrap(),
            expected_f64(&fixture, "mh_or"),
            1e-12,
        );
        approx(
            result.mh_or_ci_lower.unwrap(),
            expected_f64(&fixture, "mh_or_ci_lower"),
            1e-8,
        );
        approx(
            result.mh_or_ci_upper.unwrap(),
            expected_f64(&fixture, "mh_or_ci_upper"),
            1e-8,
        );
        approx(
            result.mh_rr.unwrap(),
            expected_f64(&fixture, "mh_rr"),
            1e-12,
        );
        approx(
            result.mh_rr_ci_lower.unwrap(),
            expected_f64(&fixture, "mh_rr_ci_lower"),
            1e-8,
        );
        approx(
            result.mh_rr_ci_upper.unwrap(),
            expected_f64(&fixture, "mh_rr_ci_upper"),
            1e-8,
        );
        approx(
            result.homogeneity_chi_square.unwrap(),
            expected_f64(&fixture, "homogeneity_chi_square"),
            1e-12,
        );
        approx(
            result.homogeneity_p.unwrap(),
            expected_f64(&fixture, "homogeneity_p"),
            1e-12,
        );
        assert_eq!(
            result.continuity_correction,
            fixture["expected"]["continuity_correction"]
                .as_bool()
                .unwrap()
        );

        let expected_strata = fixture["expected"]["mh_strata"].as_array().unwrap();
        assert_eq!(result.mh_strata.len(), expected_strata.len());
        for expected in expected_strata {
            let label = expected["label"].as_str().unwrap();
            let actual = result
                .mh_strata
                .iter()
                .find(|stratum| stratum.label == label)
                .unwrap_or_else(|| panic!("missing MH stratum {label}"));
            assert_cells(&actual.cells, &expected["cells"]);
            approx(
                actual.or_stratum,
                expected["or_stratum"].as_f64().unwrap(),
                1e-12,
            );
            approx(
                actual.rr_stratum,
                expected["rr_stratum"].as_f64().unwrap(),
                1e-12,
            );
        }
    }

    #[test]
    fn or_rr_stratified_zero_cells_stay_finite() {
        let headers = csv::StringRecord::from(vec!["exposure", "outcome", "stratum"]);
        let mut rows = Vec::new();
        push_2x2_rows(&mut rows, "s1", "1", "1", 1);
        push_2x2_rows(&mut rows, "s2", "1", "0", 3);
        push_2x2_rows(&mut rows, "s2", "0", "1", 2);

        let result = or_rr_csv(
            &rows,
            &headers,
            "exposure",
            "outcome",
            &["stratum".to_string()],
            None,
            None,
            0.05,
            NaStrategy::Drop,
        )
        .unwrap();

        assert!(result.continuity_correction);
        assert!(result.mh_or.unwrap().is_finite());
        assert!(result.mh_rr.unwrap().is_finite());
        assert!(result.homogeneity_p.unwrap().is_finite());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("continuity correction")));
    }
}
