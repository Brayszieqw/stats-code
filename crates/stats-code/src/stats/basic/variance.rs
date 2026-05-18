use std::collections::BTreeMap;

use crate::cli::NaStrategy;
use crate::math::f_distribution_p_value;
use crate::schema::{GroupVarianceSummary, VarianceHomogeneityResult};

use super::common::*;

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
