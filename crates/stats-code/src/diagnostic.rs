use std::collections::BTreeMap;
use std::path::Path;

use crate::cli::DiagnosticRocArgs;
use crate::helpers::{require_column, stringify_error};
use crate::schema::{is_missing_value, DiagnosticRocResult, DiagnosticThresholdMetrics, RocPoint};

#[derive(Debug, Clone, Copy)]
struct DiagnosticRecord {
    truth: bool,
    score: f64,
}

pub fn diagnostic_roc_csv(
    path: &Path,
    analysis_path: Option<&Path>,
    args: &DiagnosticRocArgs,
) -> Result<DiagnosticRocResult, String> {
    if let Some(threshold) = args.threshold {
        if !threshold.is_finite() {
            return Err("ROC threshold must be a finite number.".to_string());
        }
    }

    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader.headers().map_err(stringify_error)?.clone();
    let index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect();
    let truth_idx = require_column(&index, &args.truth)?;
    let score_idx = require_column(&index, &args.score)?;

    let mut n_total = 0usize;
    let mut n_excluded_missing = 0usize;
    let mut n_excluded_invalid = 0usize;
    let mut records = Vec::new();

    for record in reader.records() {
        n_total += 1;
        let record = record.map_err(stringify_error)?;
        let truth_raw = record.get(truth_idx).unwrap_or("").trim();
        let score_raw = record.get(score_idx).unwrap_or("").trim();

        if is_missing_value(truth_raw) || is_missing_value(score_raw) {
            n_excluded_missing += 1;
            continue;
        }

        let Some(truth) = parse_truth(truth_raw) else {
            n_excluded_invalid += 1;
            continue;
        };
        let Ok(score) = score_raw.parse::<f64>() else {
            n_excluded_invalid += 1;
            continue;
        };
        if !score.is_finite() {
            n_excluded_invalid += 1;
            continue;
        }

        records.push(DiagnosticRecord { truth, score });
    }

    if records.is_empty() {
        return Err("ROC analysis has no usable records after exclusions.".to_string());
    }

    let n_cases = records.iter().filter(|record| record.truth).count();
    let n_controls = records.len() - n_cases;
    if n_cases == 0 || n_controls == 0 {
        return Err(format!(
            "ROC analysis requires at least one positive case and one negative control; got cases={n_cases}, controls={n_controls}."
        ));
    }

    let auc = auc_rank_based(&records);
    let roc_points = roc_points(&records);
    let youden = best_youden_threshold(&roc_points, &records)?;
    let threshold_metrics = args
        .threshold
        .map(|threshold| threshold_metrics(&records, threshold));

    let mut warnings = Vec::new();
    if threshold_metrics
        .as_ref()
        .is_some_and(|metrics| metrics.ppv == 0.0 && metrics.tp + metrics.fp == 0)
    {
        warnings.push(
            "PPV is undefined because no records were classified positive at the requested threshold; reported as 0.0."
                .to_string(),
        );
    }
    if threshold_metrics
        .as_ref()
        .is_some_and(|metrics| metrics.npv == 0.0 && metrics.tn + metrics.fn_count == 0)
    {
        warnings.push(
            "NPV is undefined because no records were classified negative at the requested threshold; reported as 0.0."
                .to_string(),
        );
    }

    Ok(DiagnosticRocResult {
        status: "ok".to_string(),
        data_path: path.display().to_string(),
        analysis_path: analysis_path.map(|path| path.display().to_string()),
        truth: args.truth.clone(),
        score: args.score.clone(),
        n_total,
        n_used: records.len(),
        n_excluded_missing,
        n_excluded_invalid,
        n_cases,
        n_controls,
        auc,
        roc_points,
        youden,
        threshold_metrics,
        notes: vec![
            "Truth accepts 1/0, true/false, and yes/no; higher scores are treated as more likely positive."
                .to_string(),
            "AUC is Mann-Whitney/rank-based with average ranks for tied scores.".to_string(),
            "Threshold metrics classify records as positive when score >= threshold.".to_string(),
        ],
        warnings,
    })
}

fn parse_truth(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "positive" | "pos" | "case" | "disease" => Some(true),
        "0" | "false" | "no" | "n" | "negative" | "neg" | "control" | "healthy" => Some(false),
        value => value.parse::<f64>().ok().and_then(|number| {
            if (number - 1.0).abs() < f64::EPSILON {
                Some(true)
            } else if number.abs() < f64::EPSILON {
                Some(false)
            } else {
                None
            }
        }),
    }
}

fn auc_rank_based(records: &[DiagnosticRecord]) -> f64 {
    let mut sorted = records.to_vec();
    sorted.sort_by(|left, right| left.score.total_cmp(&right.score));

    let mut rank_sum_cases = 0.0;
    let mut i = 0usize;
    while i < sorted.len() {
        let mut j = i + 1;
        while j < sorted.len() && sorted[i].score.total_cmp(&sorted[j].score).is_eq() {
            j += 1;
        }
        let average_rank = (i + 1 + j) as f64 / 2.0;
        let cases_in_tie = sorted[i..j].iter().filter(|record| record.truth).count();
        rank_sum_cases += average_rank * cases_in_tie as f64;
        i = j;
    }

    let n_cases = records.iter().filter(|record| record.truth).count() as f64;
    let n_controls = records.len() as f64 - n_cases;
    let u = rank_sum_cases - n_cases * (n_cases + 1.0) / 2.0;
    (u / (n_cases * n_controls)).clamp(0.0, 1.0)
}

fn unique_thresholds_desc(records: &[DiagnosticRecord]) -> Vec<f64> {
    let mut thresholds = records
        .iter()
        .map(|record| record.score)
        .collect::<Vec<_>>();
    thresholds.sort_by(|left, right| right.total_cmp(left));
    thresholds.dedup_by(|left, right| left.total_cmp(right).is_eq());
    thresholds
}

fn roc_points(records: &[DiagnosticRecord]) -> Vec<RocPoint> {
    unique_thresholds_desc(records)
        .into_iter()
        .map(|threshold| {
            let metrics = threshold_metrics(records, threshold);
            RocPoint {
                threshold,
                sensitivity: metrics.sensitivity,
                specificity: metrics.specificity,
                false_positive_rate: 1.0 - metrics.specificity,
                true_positive_rate: metrics.sensitivity,
            }
        })
        .collect()
}

fn threshold_metrics(records: &[DiagnosticRecord], threshold: f64) -> DiagnosticThresholdMetrics {
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut tn = 0usize;
    let mut fn_count = 0usize;

    for record in records {
        let predicted_positive = record.score >= threshold;
        match (record.truth, predicted_positive) {
            (true, true) => tp += 1,
            (false, true) => fp += 1,
            (false, false) => tn += 1,
            (true, false) => fn_count += 1,
        }
    }

    let sensitivity = ratio(tp, tp + fn_count);
    let specificity = ratio(tn, tn + fp);
    let ppv = ratio(tp, tp + fp);
    let npv = ratio(tn, tn + fn_count);
    let accuracy = ratio(tp + tn, records.len());
    let balanced_accuracy = f64::midpoint(sensitivity, specificity);
    let f1_score = ratio(2 * tp, 2 * tp + fp + fn_count);
    let positive_likelihood_ratio = finite_ratio(sensitivity, 1.0 - specificity);
    let negative_likelihood_ratio = finite_ratio(1.0 - sensitivity, specificity);
    let diagnostic_odds_ratio = positive_likelihood_ratio.and_then(|positive| {
        negative_likelihood_ratio.and_then(|negative| finite_ratio(positive, negative))
    });
    let youden_j = sensitivity + specificity - 1.0;

    DiagnosticThresholdMetrics {
        threshold,
        tp,
        fp,
        tn,
        fn_count,
        sensitivity,
        specificity,
        ppv,
        npv,
        accuracy,
        balanced_accuracy,
        f1_score,
        positive_likelihood_ratio,
        negative_likelihood_ratio,
        diagnostic_odds_ratio,
        youden_j,
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn finite_ratio(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator == 0.0 {
        None
    } else {
        let value = numerator / denominator;
        value.is_finite().then_some(value)
    }
}

fn best_youden_threshold(
    points: &[RocPoint],
    records: &[DiagnosticRecord],
) -> Result<DiagnosticThresholdMetrics, String> {
    points
        .iter()
        .map(|point| threshold_metrics(records, point.threshold))
        .max_by(|left, right| {
            left.youden_j
                .total_cmp(&right.youden_j)
                .then_with(|| left.accuracy.total_cmp(&right.accuracy))
                .then_with(|| left.threshold.total_cmp(&right.threshold))
        })
        .ok_or_else(|| "ROC analysis could not generate threshold points.".to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_csv(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stats-code-diagnostic-{name}-{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::write(&path, content).expect("write csv");
        path
    }

    fn roc_args(path: &std::path::Path, threshold: Option<f64>) -> DiagnosticRocArgs {
        DiagnosticRocArgs {
            data: Some(path.to_path_buf()),
            analysis: None,
            truth: "disease".to_string(),
            score: "pred_score".to_string(),
            threshold,
        }
    }

    #[test]
    fn auc_is_one_for_perfect_separation() {
        let path = temp_csv(
            "perfect",
            "disease,pred_score\n0,0.1\n0,0.2\n1,0.8\n1,0.9\n",
        );
        let result = diagnostic_roc_csv(&path, None, &roc_args(&path, Some(0.5))).expect("roc");

        assert!((result.auc - 1.0).abs() < 1e-12);
        assert_eq!(result.youden.tp, 2);
        assert_eq!(result.youden.tn, 2);
        let threshold_metrics = result.threshold_metrics.expect("threshold metrics");
        assert_eq!(threshold_metrics.tp, 2);
        assert_eq!(threshold_metrics.fp, 0);
        assert!((threshold_metrics.accuracy - 1.0).abs() < 1e-12);
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn threshold_metrics_include_likelihood_ratios_and_f1() {
        let path = temp_csv(
            "threshold-metrics",
            "disease,pred_score\n1,0.9\n1,0.7\n1,0.2\n0,0.8\n0,0.4\n0,0.1\n",
        );
        let result = diagnostic_roc_csv(&path, None, &roc_args(&path, Some(0.5))).expect("roc");
        let metrics = result.threshold_metrics.expect("threshold metrics");

        assert_eq!(metrics.tp, 2);
        assert_eq!(metrics.fp, 1);
        assert_eq!(metrics.tn, 2);
        assert_eq!(metrics.fn_count, 1);
        assert!((metrics.balanced_accuracy - 2.0 / 3.0).abs() < 1e-12);
        assert!((metrics.f1_score - 2.0 / 3.0).abs() < 1e-12);
        assert!((metrics.positive_likelihood_ratio.expect("lr+") - 2.0).abs() < 1e-12);
        assert!((metrics.negative_likelihood_ratio.expect("lr-") - 0.5).abs() < 1e-12);
        assert!((metrics.diagnostic_odds_ratio.expect("dor") - 4.0).abs() < 1e-12);
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn auc_handles_reversed_scores_and_ties() {
        let path = temp_csv(
            "reversed",
            "disease,pred_score\n1,0.1\n1,0.2\n0,0.8\n0,0.8\n",
        );
        let result = diagnostic_roc_csv(&path, None, &roc_args(&path, None)).expect("roc");

        assert!(result.auc >= 0.0);
        assert!(result.auc < 0.1);
        assert_eq!(result.roc_points.len(), 3);
        fs::remove_file(path).expect("cleanup");
    }
}
