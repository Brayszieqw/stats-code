use crate::cli::NaStrategy;
use crate::helpers::require_column;
use crate::math::normal_cdf;
use crate::schema::{MetaAnalysisResult, MetaStudy};

use super::common::{
    check_missing_policy, chi_square_p_value, column_index, missing, parse_num, prelude_notes,
    z_critical, EPS,
};

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
                    std::string::ToString::to_string,
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
