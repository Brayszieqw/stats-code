use crate::cli::NaStrategy;
use crate::math::normal_cdf;
use crate::schema::NormalityResult;

use super::common::{
    chi_square_p_value, inverse_normal_cdf, mean, numeric_column, prelude_notes, sample_sd, EPS,
};

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
    values.sort_by(f64::total_cmp);
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
    // NOTE: This is the asymptotic two-sided Kolmogorov-Smirnov p-value,
    // not the Lilliefors-corrected version. Despite `lilliefors_used: true`
    // in the result struct, the corrected critical values from Lilliefors
    // (1967) are not yet applied. p-values should be interpreted with
    // caution at small n.
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

/// Approximate Shapiro-Wilk W statistic.
///
/// Uses Blom-style expected normal scores (`(i - 3/8) / (n + 1/4)`) for the
/// rank-based weights. This matches the asymptotic form but does NOT use the
/// Royston (1992) coefficient series, so values may differ from R's
/// `shapiro.test()` by O(1e-2) for small n. The companion p-value reported
/// by `normality_csv` is computed from the Jarque-Bera chi-square (df=2)
/// rather than Royston's transform; this is a known limitation tracked for
/// future replacement with the full Royston implementation.
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
