// ---------------------------------------------------------------------------
// Paired and one-sample t-test implementations.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;

use crate::helpers::require_column;
use crate::math::{t_distribution_critical_value, t_distribution_p_value};
use crate::schema::{TtestOneSampleResult, TtestPairedResult};

// ---------------------------------------------------------------------------
// Paired t-test
// ---------------------------------------------------------------------------

/// Compute a paired t-test from CSV rows.
///
/// Computes differences `d_i = after_i - before_i` for every complete pair,
/// then tests H0: mean difference = 0.
pub(crate) fn paired_ttest_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    col_before: &str,
    col_after: &str,
    alpha: f64,
) -> Result<TtestPairedResult, String> {
    // Build column index
    let index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect();

    let idx_before = require_column(&index, col_before)?;
    let idx_after = require_column(&index, col_after)?;

    let n_total = rows.len();

    // Collect differences for complete pairs
    let mut diffs: Vec<f64> = Vec::new();
    for row in rows {
        let raw_before = row.get(idx_before).unwrap_or("").trim();
        let raw_after = row.get(idx_after).unwrap_or("").trim();

        if raw_before.is_empty() || raw_after.is_empty() {
            continue;
        }

        let before: f64 = raw_before
            .parse()
            .map_err(|_| format!("Non-numeric value `{raw_before}` in column `{col_before}`"))?;
        let after: f64 = raw_after
            .parse()
            .map_err(|_| format!("Non-numeric value `{raw_after}` in column `{col_after}`"))?;

        diffs.push(after - before);
    }

    let n_pairs = diffs.len();
    let n_excluded_missing = n_total - n_pairs;

    if n_pairs < 2 {
        return Err(format!(
            "Paired t-test requires at least 2 complete pairs, but only {n_pairs} found \
             (total rows: {n_total}, excluded missing: {n_excluded_missing})."
        ));
    }

    // Compute statistics
    let n = n_pairs as f64;
    let mean_diff = diffs.iter().sum::<f64>() / n;
    let variance: f64 = diffs.iter().map(|d| (d - mean_diff).powi(2)).sum::<f64>() / (n - 1.0);

    if variance <= 0.0 {
        return Err(format!(
            "Paired t-test: all differences are identical (mean_diff = {mean_diff}). \
             Cannot compute test — variance of differences is zero."
        ));
    }

    let sd_diff = variance.sqrt();
    let se_diff = sd_diff / n.sqrt();
    let t_statistic = mean_diff / se_diff;
    let df = n - 1.0;
    let p_value = t_distribution_p_value(t_statistic, df);
    let t_crit = t_distribution_critical_value(alpha, df);
    let ci_lower = mean_diff - t_crit * se_diff;
    let ci_upper = mean_diff + t_crit * se_diff;

    Ok(TtestPairedResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total,
        n_used: n_pairs,
        n_excluded_missing,
        notes: vec![],
        warnings: vec![],
        method: "Paired t-test".to_string(),
        before_variable: col_before.to_string(),
        after_variable: col_after.to_string(),
        n_pairs,
        mean_diff,
        sd_diff,
        se_diff,
        t_statistic,
        df,
        p_value,
        ci_lower,
        ci_upper,
        alpha,
    })
}

// ---------------------------------------------------------------------------
// One-sample t-test
// ---------------------------------------------------------------------------

/// Compute a one-sample t-test from CSV rows.
///
/// Tests H0: population mean = `mu` against the alternative that it differs.
pub(crate) fn one_sample_ttest_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    col_var: &str,
    mu: f64,
    alpha: f64,
) -> Result<TtestOneSampleResult, String> {
    // Build column index
    let index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_string(), i))
        .collect();

    let idx_var = require_column(&index, col_var)?;

    let n_total = rows.len();

    // Collect non-missing values
    let mut values: Vec<f64> = Vec::new();
    for row in rows {
        let raw = row.get(idx_var).unwrap_or("").trim();
        if raw.is_empty() {
            continue;
        }
        let val: f64 = raw
            .parse()
            .map_err(|_| format!("Non-numeric value `{raw}` in column `{col_var}`"))?;
        values.push(val);
    }

    let n = values.len();
    let n_excluded_missing = n_total - n;

    if n < 2 {
        return Err(format!(
            "One-sample t-test requires at least 2 observations, but only {n} found \
             (total rows: {n_total}, excluded missing: {n_excluded_missing})."
        ));
    }

    // Compute statistics
    let n_f = n as f64;
    let sample_mean = values.iter().sum::<f64>() / n_f;
    let variance: f64 = values
        .iter()
        .map(|v| (v - sample_mean).powi(2))
        .sum::<f64>()
        / (n_f - 1.0);

    if variance <= 0.0 {
        return Err(format!(
            "One-sample t-test: all values are identical (mean = {sample_mean}). \
             Cannot compute test — variance is zero."
        ));
    }

    let sample_sd = variance.sqrt();
    let se = sample_sd / n_f.sqrt();
    let t_statistic = (sample_mean - mu) / se;
    let df = n_f - 1.0;
    let p_value = t_distribution_p_value(t_statistic, df);
    let t_crit = t_distribution_critical_value(alpha, df);
    let ci_lower = sample_mean - t_crit * se;
    let ci_upper = sample_mean + t_crit * se;

    Ok(TtestOneSampleResult {
        status: "ok".to_string(),
        data_path: String::new(),
        analysis_path: None,
        n_total,
        n_used: n,
        n_excluded_missing,
        notes: vec![],
        warnings: vec![],
        method: "One-sample t-test".to_string(),
        variable: col_var.to_string(),
        hypothesized_mean: mu,
        n,
        sample_mean,
        sample_sd,
        se,
        t_statistic,
        df,
        p_value,
        ci_lower,
        ci_upper,
        alpha,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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

    fn paired_rows_from_fixture(fixture: &Value) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let before = fixture["before"].as_array().unwrap();
        let after = fixture["after"].as_array().unwrap();
        assert_eq!(before.len(), after.len());
        let rows = before
            .iter()
            .zip(after.iter())
            .map(|(b, a)| {
                csv::StringRecord::from(vec![
                    b.as_f64().unwrap().to_string(),
                    a.as_f64().unwrap().to_string(),
                ])
            })
            .collect();
        (rows, csv::StringRecord::from(vec!["before", "after"]))
    }

    fn one_sample_rows_from_fixture(
        fixture: &Value,
    ) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let rows = fixture["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| csv::StringRecord::from(vec![value.as_f64().unwrap().to_string()]))
            .collect();
        (rows, csv::StringRecord::from(vec!["val"]))
    }

    // -----------------------------------------------------------------------
    // Paired t-test tests
    // -----------------------------------------------------------------------

    /// Build a simple in-memory CSV dataset for testing.
    fn make_csv(records: &[(&str, &str)]) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let headers = csv::StringRecord::from(vec!["before", "after"]);
        let rows: Vec<csv::StringRecord> = records
            .iter()
            .map(|(b, a)| csv::StringRecord::from(vec![*b, *a]))
            .collect();
        (rows, headers)
    }

    /// Build a one-column CSV dataset for one-sample tests.
    fn make_one_col_csv(records: &[&str]) -> (Vec<csv::StringRecord>, csv::StringRecord) {
        let headers = csv::StringRecord::from(vec!["val"]);
        let rows: Vec<csv::StringRecord> = records
            .iter()
            .map(|v| csv::StringRecord::from(vec![*v]))
            .collect();
        (rows, headers)
    }

    #[test]
    fn paired_ttest_matches_r_fixture() {
        let fixture = load_fixture("tests/fixtures/r/ttest_paired.json");
        let (rows, headers) = paired_rows_from_fixture(&fixture);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05).unwrap();

        assert_eq!(result.n_pairs, expected_usize(&fixture, "n_pairs"));
        assert!((result.mean_diff - expected_f64(&fixture, "mean_diff")).abs() < 1e-12);
        assert!((result.sd_diff - expected_f64(&fixture, "sd_diff")).abs() < 1e-12);
        assert!((result.se_diff - expected_f64(&fixture, "se_diff")).abs() < 1e-12);
        assert!((result.t_statistic - expected_f64(&fixture, "t_statistic")).abs() < 1e-12);
        assert!((result.df - expected_f64(&fixture, "df")).abs() < 1e-12);
        assert!((result.p_value - expected_f64(&fixture, "p_value")).abs() < 1e-6);
        assert!((result.ci_lower - expected_f64(&fixture, "ci_lower")).abs() < 1e-5);
        assert!((result.ci_upper - expected_f64(&fixture, "ci_upper")).abs() < 1e-5);
    }

    #[test]
    fn one_sample_ttest_matches_scipy_fixture() {
        let fixture = load_fixture("tests/fixtures/python/ttest_one_sample.json");
        let (rows, headers) = one_sample_rows_from_fixture(&fixture);
        let mu = fixture["mu"].as_f64().unwrap();

        let result = one_sample_ttest_csv(&rows, &headers, "val", mu, 0.05).unwrap();

        assert_eq!(result.n, expected_usize(&fixture, "n"));
        assert!((result.sample_mean - expected_f64(&fixture, "sample_mean")).abs() < 1e-12);
        assert!((result.sample_sd - expected_f64(&fixture, "sample_sd")).abs() < 1e-12);
        assert!((result.se - expected_f64(&fixture, "se")).abs() < 1e-12);
        assert!((result.t_statistic - expected_f64(&fixture, "t_statistic")).abs() < 1e-12);
        assert!((result.df - expected_f64(&fixture, "df")).abs() < 1e-12);
        assert!((result.p_value - expected_f64(&fixture, "p_value")).abs() < 1e-6);
        assert!((result.ci_lower - expected_f64(&fixture, "ci_lower")).abs() < 1e-5);
        assert!((result.ci_upper - expected_f64(&fixture, "ci_upper")).abs() < 1e-5);
    }

    #[test]
    fn paired_ttest_simple() {
        // before = [10, 12, 14, 16, 18], after = [12, 14, 15, 18, 20]
        // diffs = [2, 2, 1, 2, 2]
        // mean_diff = 1.8, sd_diff = 0.4472136, se_diff = 0.2, t = 9.0
        // df = 4, two-sided p ~ 0.00085 (from R: t.test(after, before, paired=TRUE))
        let records = vec![
            ("10", "12"),
            ("12", "14"),
            ("14", "15"),
            ("16", "18"),
            ("18", "20"),
        ];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05).unwrap();

        assert_eq!(result.n_pairs, 5);
        assert!(
            (result.mean_diff - 1.8).abs() < 1e-10,
            "mean_diff: {}",
            result.mean_diff
        );
        assert!(
            (result.sd_diff - 0.447213595).abs() < 1e-5,
            "sd_diff: {}",
            result.sd_diff
        );
        assert!(
            (result.se_diff - 0.2).abs() < 1e-10,
            "se_diff: {}",
            result.se_diff
        );
        assert!(
            (result.t_statistic - 9.0).abs() < 1e-10,
            "t: {}",
            result.t_statistic
        );
        assert_eq!(result.df, 4.0);
        assert!(result.p_value < 0.01, "p_value: {}", result.p_value);
        assert!(result.ci_lower < result.mean_diff);
        assert!(result.ci_upper > result.mean_diff);
        assert!((result.alpha - 0.05).abs() < 1e-10);
    }

    #[test]
    fn paired_ttest_known_r_output() {
        // R: before <- c(100, 105, 110, 115, 120, 125)
        //    after  <- c(102, 108, 112, 118, 122, 130)
        //    t.test(after, before, paired=TRUE)
        //
        // diffs = [2, 3, 2, 3, 2, 5]
        // mean_diff = 2.833333, sd_diff = 1.169045, se_diff = 0.477261
        // t = 5.9367, df = 5, p = 0.001933
        let records = vec![
            ("100", "102"),
            ("105", "108"),
            ("110", "112"),
            ("115", "118"),
            ("120", "122"),
            ("125", "130"),
        ];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05).unwrap();

        assert_eq!(result.n_pairs, 6);
        assert!(
            (result.mean_diff - 2.833333).abs() < 0.001,
            "mean_diff: {}",
            result.mean_diff
        );
        assert!(
            (result.sd_diff - 1.169045).abs() < 0.005,
            "sd_diff: {}",
            result.sd_diff
        );
        assert!(
            (result.t_statistic - 5.9367).abs() < 0.01,
            "t: {}",
            result.t_statistic
        );
        assert!(
            (result.p_value - 0.001933).abs() < 0.001,
            "p: {}",
            result.p_value
        );
        // CI from R: 1.606502 to 4.060164
        assert!(
            (result.ci_lower - 1.6065).abs() < 0.01,
            "ci_lower: {}",
            result.ci_lower
        );
        assert!(
            (result.ci_upper - 4.0602).abs() < 0.01,
            "ci_upper: {}",
            result.ci_upper
        );
    }

    #[test]
    fn paired_ttest_insufficient_data() {
        // Only one pair
        let records = vec![("10", "12")];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2"));
    }

    #[test]
    fn paired_ttest_missing_data_skip() {
        // Some rows have missing before or after values
        let records = vec![
            ("10", "12"),
            ("", "14"), // missing before
            ("14", ""), // missing after
            ("16", "18"),
            ("18", "21"),
        ];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05).unwrap();

        // Only 3 complete pairs (rows 0, 3, 4)
        assert_eq!(result.n_pairs, 3);
        assert_eq!(result.n_total, 5);
        assert_eq!(result.n_excluded_missing, 2);
    }

    #[test]
    fn paired_ttest_zero_variance() {
        // All differences are zero
        let records = vec![("10", "10"), ("20", "20"), ("30", "30")];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    #[test]
    fn paired_ttest_non_numeric() {
        let records = vec![("10", "abc"), ("20", "30")];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "before", "after", 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Non-numeric"));
    }

    #[test]
    fn paired_ttest_column_not_found() {
        let records = vec![("10", "12")];
        let (rows, headers) = make_csv(&records);

        let result = paired_ttest_csv(&rows, &headers, "nonexistent", "after", 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -----------------------------------------------------------------------
    // One-sample t-test tests
    // -----------------------------------------------------------------------

    #[test]
    fn one_sample_ttest_known() {
        // Values: [10, 12, 14, 16, 18], mu = 0
        // mean = 14, sd = 3.16227766, se = 1.414214, t = 14/1.414 = 9.899
        // df = 4, two-sided p ~ 0.00058 (R: t.test(c(10,12,14,16,18), mu=0))
        let records = vec!["10", "12", "14", "16", "18"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 0.0, 0.05).unwrap();

        assert_eq!(result.n, 5);
        assert!((result.sample_mean - 14.0).abs() < 1e-10);
        assert!((result.sample_sd - 3.16227766).abs() < 1e-5);
        assert!(
            (result.t_statistic - 9.899495).abs() < 0.01,
            "t: {}",
            result.t_statistic
        );
        assert_eq!(result.df, 4.0);
        assert!(result.p_value < 0.01, "p_value: {}", result.p_value);
    }

    #[test]
    fn one_sample_ttest_mu_nonzero() {
        // Values: [10, 12, 14, 16, 18], mu = 14
        // mean = 14, diff from mu = 0, t = 0, p ~ 1.0
        let records = vec!["10", "12", "14", "16", "18"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 14.0, 0.05).unwrap();

        assert!((result.sample_mean - 14.0).abs() < 1e-10);
        assert!(
            (result.t_statistic).abs() < 1e-10,
            "t should be ~0, got {}",
            result.t_statistic
        );
        assert!(
            (result.p_value - 1.0).abs() < 1e-8,
            "p should be ~1.0, got {}",
            result.p_value
        );
    }

    #[test]
    fn one_sample_ttest_ci_coverage() {
        // Values: [100, 102, 98, 101, 99], mu = 0
        // mean = 100, and CI should contain 100
        let records = vec!["100", "102", "98", "101", "99"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 0.0, 0.05).unwrap();

        assert!(result.ci_lower < result.sample_mean);
        assert!(result.ci_upper > result.sample_mean);
        // CI should contain the true mean (100)
        assert!(
            result.ci_lower < 100.0 && result.ci_upper > 100.0,
            "CI [{}, {}] does not contain 100",
            result.ci_lower,
            result.ci_upper
        );
    }

    #[test]
    fn one_sample_ttest_insufficient_data() {
        let records = vec!["10"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 0.0, 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2"));
    }

    #[test]
    fn one_sample_ttest_missing_skip() {
        let records = vec!["10", "", "14", "", "18"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 0.0, 0.05).unwrap();

        assert_eq!(result.n, 3);
        assert_eq!(result.n_total, 5);
        assert_eq!(result.n_excluded_missing, 2);
    }

    #[test]
    fn one_sample_ttest_identical_values_error() {
        let records = vec!["5", "5", "5"];
        let (rows, headers) = make_one_col_csv(&records);

        let result = one_sample_ttest_csv(&rows, &headers, "val", 0.0, 0.05);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    // -----------------------------------------------------------------------
    // t-critical value sanity
    // -----------------------------------------------------------------------

    #[test]
    fn t_critical_known_values() {
        // For df=10, alpha=0.05, two-sided critical value ~ 2.228 (from t-table)
        let t_crit = t_distribution_critical_value(0.05, 10.0);
        assert!((t_crit - 2.228).abs() < 0.01, "t_crit(df=10): got {t_crit}");

        // For df=30, alpha=0.05, two-sided critical value ~ 2.042
        let t_crit_30 = t_distribution_critical_value(0.05, 30.0);
        assert!(
            (t_crit_30 - 2.042).abs() < 0.01,
            "t_crit(df=30): got {t_crit_30}"
        );

        // For df=1000, alpha=0.05, should be close to normal 1.96
        let t_crit_large = t_distribution_critical_value(0.05, 1000.0);
        assert!(
            (t_crit_large - 1.96).abs() < 0.01,
            "t_crit(df=1000): got {t_crit_large}"
        );

        // For df=1, alpha=0.05, two-sided critical value ~ 12.706
        let t_crit_df1 = t_distribution_critical_value(0.05, 1.0);
        assert!(
            (t_crit_df1 - 12.706).abs() < 0.05,
            "t_crit(df=1): got {t_crit_df1}"
        );
    }
}
