// ---------------------------------------------------------------------------
// Correlation analysis: Pearson r and Spearman 蟻 (Req 8)
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;

use crate::helpers::require_column;
use crate::math::rank_with_ties;
use crate::schema::{is_missing_value_for_column, CorrelationResult};

// ---------------------------------------------------------------------------
// helper: extract numeric values from a data column
// ---------------------------------------------------------------------------

/// Parse numeric values from a CSV column by index. Returns `None` for rows
/// where the value is missing / blank / non-numeric.
fn parse_numeric_column(
    rows: &[csv::StringRecord],
    col_idx: usize,
    col_name: &str,
) -> Vec<Option<f64>> {
    rows.iter()
        .map(|row| {
            let raw = row.get(col_idx).unwrap_or("");
            if is_missing_value_for_column(col_name, raw) {
                return None;
            }
            raw.trim().parse::<f64>().ok()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// inverse normal CDF (probit / normal quantile function)
// ---------------------------------------------------------------------------

/// Approximate inverse normal CDF (quantile function).
///
/// Uses a rational approximation (Abramowitz-Stegun 26.2.23) accurate to ~4
/// decimal places in the central 90% range, which is sufficient for CI
/// back-transformation.
fn norm_quantile(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }

    // Work with the lower tail
    let lower = if p > 0.5 { 1.0 - p } else { p };
    let t = (-2.0 * lower.ln()).sqrt();

    let c0 = 2.515_517;
    let c1 = 0.802_853;
    let c2 = 0.010_328;
    let d1 = 1.432_788;
    let d2 = 0.189_269;
    let d3 = 0.001_308;

    let num = c0 + c1 * t + c2 * t * t;
    let den = 1.0 + d1 * t + d2 * t * t + d3 * t * t * t;
    let z = t - num / den;

    if p < 0.5 {
        -z
    } else {
        z
    }
}

// ---------------------------------------------------------------------------
// Pearson correlation
// ---------------------------------------------------------------------------

/// Compute the Pearson product-moment correlation coefficient.
fn pearson_r(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;

    for (xi, yi) in x.iter().zip(y.iter()) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom <= 0.0 || !denom.is_finite() {
        return 0.0;
    }
    (cov / denom).clamp(-1.0, 1.0)
}

// ---------------------------------------------------------------------------
// Fisher z-transform confidence interval
// ---------------------------------------------------------------------------

/// Compute confidence interval for Pearson r via Fisher z-transform.
///
/// Returns `(ci_lower, ci_upper, se_fisher_z)`.
fn fisher_z_ci(r: f64, n: usize, alpha: f64) -> (f64, f64, f64) {
    if n <= 3 || r.abs() >= 1.0 {
        if r.abs() >= 1.0 {
            return (r, r, 0.0);
        }
        return (-1.0, 1.0, f64::INFINITY);
    }

    let z_crit = norm_quantile(1.0 - alpha / 2.0); // two-sided
    let se = 1.0 / ((n - 3) as f64).sqrt();

    // Fisher z transformation: z = 0.5 * ln((1+r)/(1-r))
    // For r = -1 or r = 1, use clamping
    let r_clamped = r.clamp(-0.99999999999, 0.99999999999);
    let z = 0.5 * ((1.0 + r_clamped) / (1.0 - r_clamped)).ln();
    let z_lower = z - z_crit * se;
    let z_upper = z + z_crit * se;

    // Back-transform: r = (exp(2z) - 1) / (exp(2z) + 1)
    let back = |zv: f64| -> f64 {
        let exp2z = (2.0 * zv).exp();
        (exp2z - 1.0) / (exp2z + 1.0)
    };

    (
        back(z_lower).clamp(-1.0, 1.0),
        back(z_upper).clamp(-1.0, 1.0),
        se,
    )
}

// ---------------------------------------------------------------------------
// t-test for Pearson correlation significance
// ---------------------------------------------------------------------------

/// Two-sided p-value for Pearson r using the t-distribution.
///
/// Returns `(t_statistic, df, p_value)`.
fn pearson_t_test(r: f64, n: usize) -> (f64, f64, f64) {
    if n <= 2 {
        return (0.0, 0.0, 1.0);
    }
    let df = (n - 2) as f64;
    let denom = (1.0 - r * r).sqrt().max(1e-15);
    let t = r * (df.sqrt()) / denom;
    let p = crate::math::t_distribution_p_value(t, df);
    (t, df, p)
}

// ---------------------------------------------------------------------------
// main entry point
// ---------------------------------------------------------------------------

/// Compute Pearson or Spearman correlation from CSV rows.
///
/// # Arguments
/// * `rows` - CSV data rows (each record maps to one observation)
/// * `headers` - CSV header record (column names)
/// * `col_x` - name of the X variable column
/// * `col_y` - name of the Y variable column
/// * `alpha` - significance level for confidence intervals (e.g. 0.05 for 95%)
/// * `method` - `"pearson"` or `"spearman"`
///
/// # Errors
/// Returns `Err(...)` when:
/// - A column name is not found in the header
/// - `method` is `"spearman"` and there are fewer than 3 complete pairs
pub(crate) fn correlation_csv(
    rows: &[csv::StringRecord],
    headers: &csv::StringRecord,
    col_x: &str,
    col_y: &str,
    alpha: f64,
    method: &str,
) -> Result<CorrelationResult, String> {
    // 1. Build column index
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, hdr) in headers.iter().enumerate() {
        index.insert(hdr.to_string(), i);
    }
    let idx_x = require_column(&index, col_x)?;
    let idx_y = require_column(&index, col_y)?;

    // 2. Parse numeric columns
    let x_raw = parse_numeric_column(rows, idx_x, col_x);
    let y_raw = parse_numeric_column(rows, idx_y, col_y);

    let n_total = rows.len();

    // 3. Collect complete pairs (both values present and numeric)
    let mut x_vals: Vec<f64> = Vec::with_capacity(n_total);
    let mut y_vals: Vec<f64> = Vec::with_capacity(n_total);
    for (x_opt, y_opt) in x_raw.iter().zip(y_raw.iter()) {
        if let (Some(x_val), Some(y_val)) = (x_opt, y_opt) {
            x_vals.push(*x_val);
            y_vals.push(*y_val);
        } else { /* excluded */ }
    }

    let n_pairs = x_vals.len();
    let n_excluded_missing = n_total.saturating_sub(n_pairs);

    // 4. Spearman-specific check: refuse n < 3
    if method.eq_ignore_ascii_case("spearman") && n_pairs < 3 {
        return Err(format!(
            "Spearman correlation requires at least 3 complete pairs, \
             but only {n_pairs} were found (total rows: {n_total}, excluded missing: {n_excluded_missing})."
        ));
    }

    // 5. Compute Pearson r. For a Spearman-only request, the headline `r`
    // fields represent Pearson correlation on average ranks (rho) so callers
    // can use a single scalar field consistently.
    let pearson_raw = pearson_r(&x_vals, &y_vals);
    let mut headline_r = pearson_raw;

    // 6. Spearman (if requested)
    let (spearman_rho, spearman_p_value) = if method.eq_ignore_ascii_case("spearman") {
        let x_ranks = rank_with_ties(&x_vals);
        let y_ranks = rank_with_ties(&y_vals);
        let rho = pearson_r(&x_ranks, &y_ranks);
        let (_t_s, _df_s, p_s) = pearson_t_test(rho, n_pairs);
        headline_r = rho;
        (Some(rho), Some(p_s))
    } else {
        (None, None)
    };

    let r = headline_r;
    let r_squared = r * r;

    // 7. CI via Fisher z
    let (ci_lower, ci_upper, se_fisher_z) = fisher_z_ci(r, n_pairs, alpha);

    // 8. t-test for the headline coefficient
    let (t_statistic, df, p_value) = pearson_t_test(r, n_pairs);

    Ok(CorrelationResult {
        status: "ok".to_string(),
        data_path: String::new(), // filled by caller if needed
        analysis_path: None,      // filled by caller if needed
        n_total,
        n_used: n_pairs,
        n_excluded_missing,
        notes: vec![format!(
            "{method} correlation between {col_x} and {col_y}; \
             {n_pairs} complete pairs out of {n_total} rows."
        )],
        warnings: vec![],
        method: method.to_lowercase(),
        x_variable: col_x.to_string(),
        y_variable: col_y.to_string(),
        n_pairs,
        r,
        r_squared,
        se_fisher_z,
        ci_lower,
        ci_upper,
        t_statistic,
        df,
        p_value,
        alpha,
        spearman_rho,
        spearman_p_value,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // -----------------------------------------------------------------------
    // helpers to build test CSV data
    // -----------------------------------------------------------------------

    /// Create a simple `StringRecord` header from a slice of column names.
    fn make_headers(names: &[&str]) -> csv::StringRecord {
        let mut rec = csv::StringRecord::new();
        for name in names {
            rec.push_field(name);
        }
        rec
    }

    /// Create CSV rows from string slices (each inner slice is a row).
    fn make_rows(data: &[&[&str]]) -> Vec<csv::StringRecord> {
        data.iter()
            .map(|row| {
                let mut rec = csv::StringRecord::new();
                for field in *row {
                    rec.push_field(field);
                }
                rec
            })
            .collect()
    }

    /// Convenience wrapper that builds headers/rows, calls correlation_csv.
    fn run_correlation(
        col_names: &[&str],
        data: &[&[&str]],
        col_x: &str,
        col_y: &str,
        method: &str,
    ) -> Result<CorrelationResult, String> {
        let headers = make_headers(col_names);
        let rows = make_rows(data);
        correlation_csv(&rows, &headers, col_x, col_y, 0.05, method)
    }

    fn load_fixture(relative_path: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()))
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

    fn expected_f64(fixture: &Value, method: &str, key: &str) -> f64 {
        fixture["expected"][method][key]
            .as_f64()
            .unwrap_or_else(|| panic!("missing expected.{method}.{key}"))
    }

    fn expected_usize(fixture: &Value, method: &str, key: &str) -> usize {
        fixture["expected"][method][key]
            .as_u64()
            .unwrap_or_else(|| panic!("missing expected.{method}.{key}")) as usize
    }

    fn approx(actual: f64, expected: f64, tol: f64) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual}"
        );
    }

    // -----------------------------------------------------------------------
    // unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn pearson_matches_scipy_fixture() {
        let fixture = load_fixture("tests/fixtures/python/correlation_pearson_spearman.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["x", "y"]);

        let result = correlation_csv(&rows, &headers, "x", "y", 0.05, "pearson").unwrap();

        assert_eq!(
            result.n_pairs,
            expected_usize(&fixture, "pearson", "n_pairs")
        );
        approx(result.r, expected_f64(&fixture, "pearson", "r"), 1e-12);
        approx(
            result.r_squared,
            expected_f64(&fixture, "pearson", "r_squared"),
            1e-12,
        );
        approx(
            result.se_fisher_z,
            expected_f64(&fixture, "pearson", "se_fisher_z"),
            1e-12,
        );
        approx(
            result.ci_lower,
            expected_f64(&fixture, "pearson", "ci_lower"),
            1e-4,
        );
        approx(
            result.ci_upper,
            expected_f64(&fixture, "pearson", "ci_upper"),
            1e-4,
        );
        approx(
            result.t_statistic,
            expected_f64(&fixture, "pearson", "t_statistic"),
            1e-12,
        );
        approx(result.df, expected_f64(&fixture, "pearson", "df"), 1e-12);
        approx(
            result.p_value,
            expected_f64(&fixture, "pearson", "p_value"),
            1e-12,
        );
        assert!(result.spearman_rho.is_none());
        assert!(result.spearman_p_value.is_none());
    }

    #[test]
    fn spearman_matches_scipy_fixture() {
        let fixture = load_fixture("tests/fixtures/python/correlation_pearson_spearman.json");
        let (rows, headers) = rows_from_fixture(&fixture, &["x", "y"]);

        let result = correlation_csv(&rows, &headers, "x", "y", 0.05, "spearman").unwrap();

        assert_eq!(
            result.n_pairs,
            expected_usize(&fixture, "spearman", "n_pairs")
        );
        approx(result.r, expected_f64(&fixture, "spearman", "r"), 1e-12);
        approx(
            result.r_squared,
            expected_f64(&fixture, "spearman", "r_squared"),
            1e-12,
        );
        approx(
            result.se_fisher_z,
            expected_f64(&fixture, "spearman", "se_fisher_z"),
            1e-12,
        );
        approx(
            result.ci_lower,
            expected_f64(&fixture, "spearman", "ci_lower"),
            1e-4,
        );
        approx(
            result.ci_upper,
            expected_f64(&fixture, "spearman", "ci_upper"),
            1e-4,
        );
        approx(
            result.t_statistic,
            expected_f64(&fixture, "spearman", "t_statistic"),
            1e-12,
        );
        approx(result.df, expected_f64(&fixture, "spearman", "df"), 1e-12);
        approx(
            result.p_value,
            expected_f64(&fixture, "spearman", "p_value"),
            1e-12,
        );
        approx(
            result.spearman_rho.unwrap(),
            expected_f64(&fixture, "spearman", "spearman_rho"),
            1e-12,
        );
        approx(
            result.spearman_p_value.unwrap(),
            expected_f64(&fixture, "spearman", "spearman_p_value"),
            1e-12,
        );
    }

    #[test]
    fn pearson_perfect_positive() {
        // y = 2x + 1  => perfect linear positive correlation
        // NOTE: avoid data values 9, 99, 999 etc. (SAS sentinel missing codes)
        let data: &[&[&str]] = &[
            &["10", "21"],
            &["11", "23"],
            &["12", "25"],
            &["13", "27"],
            &["14", "29"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        assert!(
            (result.r - 1.0).abs() < 1e-10,
            "expected r=1.0, got {}",
            result.r
        );
        assert!((result.r_squared - 1.0).abs() < 1e-10);
        assert_eq!(result.n_pairs, 5);
        assert_eq!(result.method, "pearson");
        assert!(result.spearman_rho.is_none());
        assert!(result.spearman_p_value.is_none());
    }

    #[test]
    fn pearson_perfect_negative() {
        // y = -2x + 10  => perfect linear negative correlation
        let data: &[&[&str]] = &[
            &["1", "8"],
            &["2", "6"],
            &["3", "4"],
            &["4", "2"],
            &["5", "0"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        assert!(
            (result.r - (-1.0)).abs() < 1e-10,
            "expected r=-1.0, got {}",
            result.r
        );
        assert!((result.r_squared - 1.0).abs() < 1e-10);
        assert_eq!(result.n_pairs, 5);
    }

    #[test]
    fn pearson_uncorrelated() {
        // Independent variables, r should be close to 0
        let data: &[&[&str]] = &[
            &["1", "5"],
            &["2", "2"],
            &["3", "8"],
            &["4", "1"],
            &["5", "7"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        // With only 5 random-ish points, r won't be exactly 0
        assert!(
            result.r.abs() < 0.8,
            "expected near-zero r, got {}",
            result.r
        );
    }

    #[test]
    fn pearson_fisher_ci_coverage() {
        // With n=30, r=0 => SE(z) = 1/sqrt(n-3) = 1/sqrt(27)
        // Use x values 10..39 to avoid SAS sentinel 9
        let mut fields: Vec<Vec<String>> = Vec::new();
        for i in 10..40 {
            fields.push(vec![i.to_string(), "0".to_string()]);
        }
        let data: Vec<&[String]> = fields.iter().map(|v| v.as_slice()).collect();
        let headers = make_headers(&["x", "y"]);
        let rows: Vec<csv::StringRecord> = data
            .iter()
            .map(|row| {
                let mut rec = csv::StringRecord::new();
                for f in *row {
                    rec.push_field(f);
                }
                rec
            })
            .collect();

        let result = correlation_csv(&rows, &headers, "x", "y", 0.05, "pearson").unwrap();
        assert!((result.se_fisher_z - (1.0 / 27.0_f64.sqrt())).abs() < 1e-10);
        assert_eq!(result.n_pairs, 30);
    }

    #[test]
    fn spearman_perfect_monotonic() {
        // y = x^2  => monotonic but not linear
        // Avoid SAS sentinel 9 by using offset
        let data: &[&[&str]] = &[
            &["10", "100"],
            &["11", "121"],
            &["12", "144"],
            &["13", "169"],
            &["14", "196"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "spearman").unwrap();
        // Perfect monotonic increasing => rho = 1.0
        assert!(
            (result.spearman_rho.unwrap() - 1.0).abs() < 1e-10,
            "expected rho=1.0, got {:?}",
            result.spearman_rho
        );
        assert!(result.spearman_p_value.unwrap() < 0.05);
        assert_eq!(result.method, "spearman");
        // r (Pearson on ranks) should also be 1.0
        assert!((result.r - 1.0).abs() < 1e-10);
    }

    #[test]
    fn spearman_with_ties() {
        // Data with ties: x has a duplicate (2,2), y has monotonic trend
        let data: &[&[&str]] = &[
            &["1", "1"],
            &["2", "4"],
            &["2", "3"], // tie in x
            &["3", "16"],
            &["4", "25"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "spearman").unwrap();
        // Should still produce a valid rho
        let rho = result.spearman_rho.unwrap();
        assert!(rho.is_finite(), "rho should be finite, got {rho}");
        assert!(rho > 0.8, "expected high positive rho, got {rho}");
    }

    #[test]
    fn spearman_too_few_pairs() {
        // Only 2 complete pairs => should error for Spearman
        let data: &[&[&str]] = &[&["1", "2"], &["3", "4"]];
        let result = run_correlation(&["x", "y"], data, "x", "y", "spearman");
        assert!(result.is_err(), "expected Err, got Ok");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("at least 3 complete pairs"),
            "error message should mention 3 complete pairs: {msg}"
        );
    }

    #[test]
    fn handles_missing_values() {
        // Some pairs have missing y values
        let data: &[&[&str]] = &[
            &["1", "2"],
            &["2", ""], // missing y
            &["3", "6"],
            &["", "8"], // missing x
            &["5", "10"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        // Only 3 complete pairs: (1,2), (3,6), (5,10) => r=1.0
        assert_eq!(result.n_pairs, 3);
        assert_eq!(result.n_excluded_missing, 2);
        assert_eq!(result.n_total, 5);
        assert!(result.n_used == 3);
        assert!((result.r - 1.0).abs() < 1e-10);
    }

    #[test]
    fn handles_non_numeric_missing_codes() {
        let data: &[&[&str]] = &[
            &["1", "2"],
            &["2", "NA"],
            &["3", "missing"],
            &["4", "8"],
            &["5", "10"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        // 3 complete pairs: (1,2), (4,8), (5,10) => r=1.0 (linear: y=2x)
        assert_eq!(result.n_pairs, 3);
        assert_eq!(result.n_excluded_missing, 2);
    }

    #[test]
    fn unknown_column_returns_error() {
        let data: &[&[&str]] = &[&["1", "2"]];
        let result = run_correlation(&["x", "y"], data, "z", "y", "pearson");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("z"), "error should mention column name: {msg}");
    }

    #[test]
    fn pearson_p_value_extreme() {
        // Perfect correlation with n=5 => p should be very small
        // Avoid SAS sentinel 9
        let data: &[&[&str]] = &[
            &["10", "21"],
            &["11", "23"],
            &["12", "25"],
            &["13", "27"],
            &["14", "29"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        // For perfect r with n=5, df=3, t is effectively infinite => p ~ 0
        assert!(
            result.p_value < 0.001,
            "p-value should be tiny, got {}",
            result.p_value
        );
        assert!((result.df - 3.0).abs() < 0.01);
        // t should be very large
        assert!(result.t_statistic > 10.0);
    }

    #[test]
    fn spearman_monotonic_decreasing() {
        // y decreases as x increases => monotonic decreasing
        let data: &[&[&str]] = &[
            &["1", "6"],
            &["2", "3"],
            &["3", "2"],
            &["4", "1.5"],
            &["6", "1"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "spearman").unwrap();
        let rho = result.spearman_rho.unwrap();
        assert!((rho - (-1.0)).abs() < 1e-10, "expected rho=-1.0, got {rho}");
    }

    #[test]
    fn pearson_known_result() {
        // Verified by manual calculation:
        //   x = [1, 2, 3, 4, 5], y = [2, 4, 5, 4, 5]
        //   r = 0.7745967, t = 2.1213, df = 3
        let data: &[&[&str]] = &[
            &["1", "2"],
            &["2", "4"],
            &["3", "5"],
            &["4", "4"],
            &["5", "5"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "pearson").unwrap();
        let expected_r = 0.774_596_7;
        assert!(
            (result.r - expected_r).abs() < 1e-6,
            "expected r={}, got {}",
            expected_r,
            result.r
        );
        assert!(
            (result.t_statistic - 2.121).abs() < 0.02,
            "t-stat: got {}",
            result.t_statistic
        );
        assert!((result.df - 3.0).abs() < 0.01, "df: got {}", result.df);
        assert!(
            result.p_value > 0.05,
            "p-value should be > 0.05, got {}",
            result.p_value
        );
    }

    #[test]
    fn pearson_with_alpha_01() {
        // Higher confidence (alpha=0.01) should produce wider CI
        // Avoid SAS sentinel 9 by using values 10..19
        let data: &[&[&str]] = &[
            &["10", "20"],
            &["11", "23"],
            &["12", "25"],
            &["13", "26"],
            &["14", "28"],
            &["15", "31"],
            &["16", "32"],
            &["17", "35"],
            &["18", "37"],
            &["19", "40"],
        ];
        let headers = make_headers(&["x", "y"]);
        let rows = make_rows(data);

        let result_05 = correlation_csv(&rows, &headers, "x", "y", 0.05, "pearson").unwrap();
        let result_01 = correlation_csv(&rows, &headers, "x", "y", 0.01, "pearson").unwrap();

        // 99% CI should be wider than 95% CI
        let width_05 = result_05.ci_upper - result_05.ci_lower;
        let width_01 = result_01.ci_upper - result_01.ci_lower;
        assert!(
            width_01 > width_05,
            "99% CI width {:.4} should be > 95% CI width {:.4}",
            width_01,
            width_05
        );
        assert_eq!(result_05.alpha, 0.05);
        assert_eq!(result_01.alpha, 0.01);
    }

    #[test]
    fn spearman_identical_to_pearson_for_linear_data() {
        // For perfectly linear data, Spearman rho should equal Pearson r
        let data: &[&[&str]] = &[
            &["1", "3"],
            &["2", "5"],
            &["3", "7"],
            &["4", "11"],
            &["5", "13"],
        ];
        let result = run_correlation(&["x", "y"], data, "x", "y", "spearman").unwrap();
        assert!((result.r - 1.0).abs() < 1e-10);
        assert!((result.spearman_rho.unwrap() - 1.0).abs() < 1e-10);
    }

    // -----------------------------------------------------------------------
    // rank_with_ties unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn ranks_no_ties() {
        let values = vec![3.0, 1.0, 2.0];
        let ranks = rank_with_ties(&values);
        assert_eq!(ranks, vec![3.0, 1.0, 2.0]);
    }

    #[test]
    fn ranks_with_ties() {
        let values = vec![1.0, 2.0, 2.0, 3.0];
        let ranks = rank_with_ties(&values);
        // Sorted: 1.0(idx 0), 2.0(idx 1), 2.0(idx 2), 3.0(idx 3)
        // Ranks: 1, (2+3)/2=2.5, (2+3)/2=2.5, 4
        assert_eq!(ranks, vec![1.0, 2.5, 2.5, 4.0]);
    }

    #[test]
    fn ranks_all_tied() {
        let values = vec![5.0, 5.0, 5.0];
        let ranks = rank_with_ties(&values);
        // All tied: ranks = (1+2+3)/3 = 2.0 each
        assert_eq!(ranks, vec![2.0, 2.0, 2.0]);
    }

    #[test]
    fn ranks_three_way_tie() {
        let values = vec![1.0, 3.0, 3.0, 3.0, 5.0];
        let ranks = rank_with_ties(&values);
        // Sorted: 1.0(idx 0), 3.0(idx 1), 3.0(idx 2), 3.0(idx 3), 5.0(idx 4)
        // Ranks: 1, (2+3+4)/3=3, 3, 3, 5
        assert_eq!(ranks, vec![1.0, 3.0, 3.0, 3.0, 5.0]);
    }

    // -----------------------------------------------------------------------
    // norm_quantile tests
    // -----------------------------------------------------------------------

    #[test]
    fn norm_quantile_known_values() {
        // Standard normal quantiles
        assert!((norm_quantile(0.5) - 0.0).abs() < 1e-6);
        assert!((norm_quantile(0.975) - 1.96).abs() < 0.02);
        assert!((norm_quantile(0.025) - (-1.96)).abs() < 0.02);
        assert!((norm_quantile(0.995) - 2.576).abs() < 0.02);
        assert!((norm_quantile(0.005) - (-2.576)).abs() < 0.02);
        assert!((norm_quantile(0.95) - 1.645).abs() < 0.01);
        assert!((norm_quantile(0.05) - (-1.645)).abs() < 0.01);
    }

    #[test]
    fn norm_quantile_boundaries() {
        assert!(norm_quantile(0.0).is_infinite());
        assert!(norm_quantile(1.0).is_infinite());
        assert!(norm_quantile(0.0) < 0.0);
        assert!(norm_quantile(1.0) > 0.0);
    }
}
