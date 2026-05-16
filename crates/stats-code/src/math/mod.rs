// ---------------------------------------------------------------------------
// Pure mathematical functions used across statistical models.
// ---------------------------------------------------------------------------

//! Numerical kernels shared by Stats Code models.
//!
//! The functions here are intentionally dependency-light and deterministic:
//! matrix algebra, probability distribution approximations, likelihood helpers,
//! and model-quality statistics used by logistic, Cox, linear, and table output
//! paths. Public callers should validate data shape before reaching this layer.
//!
//! Probability helpers favor stable closed-form or series approximations used in
//! standard statistical texts: the normal CDF uses an Abramowitz-Stegun style
//! approximation, gamma/beta functions use Lanczos and continued-fraction
//! routines, and matrix inversion uses Gauss-Jordan elimination with explicit
//! singular-pivot errors.

pub(crate) mod distributions;
pub(crate) mod glm;
pub(crate) mod linalg;

pub(crate) use linalg::jacobi_eigh;

/// Dot product of two vectors.
pub(crate) fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| left * right)
        .sum()
}

/// Numerically stable sigmoid function.
pub(crate) fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        let exponent = (-value).exp();
        1.0 / (1.0 + exponent)
    } else {
        let exponent = value.exp();
        exponent / (1.0 + exponent)
    }
}

/// Matrix-vector multiplication.
pub(crate) fn matrix_vector_mul(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    matrix
        .iter()
        .map(|row| {
            row.iter()
                .zip(vector.iter())
                .map(|(left, right)| left * right)
                .sum()
        })
        .collect()
}

/// Clamped exponential to avoid overflow/underflow.
pub(crate) fn safe_exp(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else if value >= 709.0 {
        f64::MAX
    } else if value <= -709.0 {
        0.0
    } else {
        value.exp()
    }
}

/// Gauss-Jordan matrix inversion.
#[allow(clippy::needless_range_loop)]
pub(crate) fn invert_matrix(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return Err("Matrix must be square for inversion.".to_string());
    }

    let mut augmented: Vec<Vec<f64>> = matrix
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut new_row = row.clone();
            new_row.extend(std::iter::repeat_n(0.0, n));
            new_row[n + i] = 1.0;
            new_row
        })
        .collect();

    for col in 0..n {
        let mut max_row = col;
        let mut max_val = augmented[col][col].abs();
        for row in (col + 1)..n {
            let val = augmented[row][col].abs();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }
        if max_val < 1e-15 {
            return Err(format!(
                "Singular matrix at column {col} (max pivot {max_val:.2e}). Check for collinearity or constant predictors."
            ));
        }
        if max_row != col {
            augmented.swap(max_row, col);
        }
        let pivot = augmented[col][col];
        for j in 0..(2 * n) {
            augmented[col][j] /= pivot;
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = augmented[row][col];
            for j in 0..(2 * n) {
                augmented[row][j] -= factor * augmented[col][j];
            }
        }
    }

    Ok(augmented.into_iter().map(|row| row[n..].to_vec()).collect())
}

/// Matrix inversion with progressive ridge regularization fallback.
pub(crate) fn invert_matrix_with_ridge(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, String> {
    if let Ok(value) = invert_matrix(matrix) {
        Ok(value)
    } else {
        let n = matrix.len();
        let mut ridge = 1e-8_f64;
        for _ in 0..8 {
            let mut regularized = matrix.to_vec();
            for (index, row) in regularized.iter_mut().enumerate().take(n) {
                row[index] += ridge;
            }
            if let Ok(value) = invert_matrix(&regularized) {
                return Ok(value);
            }
            ridge *= 10.0;
        }
        Err("Design matrix is singular; check collinearity or constant predictors.".to_string())
    }
}

// ---------------------------------------------------------------------------
// Probability distributions
// ---------------------------------------------------------------------------

/// Standard normal CDF (Abramowitz & Stegun approximation).
pub(crate) fn normal_cdf(value: f64) -> f64 {
    let absolute = value.abs();
    let t = 1.0 / (1.0 + 0.231_641_9 * absolute);
    let density = (-0.5 * absolute * absolute).exp() / 2.506_628_274_631_000_2;
    let approximation = 1.0
        - density
            * t
            * (0.319_381_530
                + t * (-0.356_563_782
                    + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429))));
    if value >= 0.0 {
        approximation
    } else {
        1.0 - approximation
    }
}

/// Chi-square CDF using regularized lower incomplete gamma function.
pub(crate) fn chi_square_cdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 || df <= 0.0 {
        return 0.0;
    }
    regularized_lower_gamma(df / 2.0, x / 2.0)
}

/// Two-sided Fisher exact test p-value for a 2x2 contingency table.
pub(crate) fn fisher_exact_2x2(a: usize, b: usize, c: usize, d: usize) -> f64 {
    let row1 = a + b;
    let row2 = c + d;
    let col1 = a + c;
    let total = row1 + row2;
    if total == 0 {
        return f64::NAN;
    }

    let min_a = col1.saturating_sub(row2);
    let max_a = row1.min(col1);
    let observed = hypergeometric_2x2_probability(a, row1, row2, col1);
    if !observed.is_finite() || observed <= 0.0 {
        return f64::NAN;
    }

    let mut p_value = 0.0;
    for candidate_a in min_a..=max_a {
        let probability = hypergeometric_2x2_probability(candidate_a, row1, row2, col1);
        if probability <= observed * (1.0 + 1e-12) {
            p_value += probability;
        }
    }
    p_value.clamp(0.0, 1.0)
}

fn hypergeometric_2x2_probability(a: usize, row1: usize, row2: usize, col1: usize) -> f64 {
    if a > row1 || col1 < a || col1 - a > row2 {
        return 0.0;
    }
    (ln_choose(row1, a) + ln_choose(row2, col1 - a) - ln_choose(row1 + row2, col1)).exp()
}

fn ln_choose(n: usize, k: usize) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    log_gamma_lanczos((n + 1) as f64)
        - log_gamma_lanczos((k + 1) as f64)
        - log_gamma_lanczos((n - k + 1) as f64)
}

/// Lanczos log-gamma with g=7 (higher precision, used by gamma/beta functions).
pub(crate) fn log_gamma_lanczos(x: f64) -> f64 {
    if x < 0.5 {
        let reflection = std::f64::consts::PI / (std::f64::consts::PI * x).sin();
        reflection.ln() - log_gamma_lanczos(1.0 - x)
    } else {
        let c = [
            0.999_999_999_999_809_9,
            676.5203681218851,
            -1259.1392167224028,
            771.323_428_777_653_1,
            -176.615_029_162_140_6,
            12.507343278686905,
            -0.13857109526572012,
            9.984_369_578_019_572e-6,
            1.5056327351493116e-7,
        ];
        let xx = x - 1.0;
        let t = xx + 7.5;
        let mut s = c[0];
        for (i, coefficient) in c.iter().enumerate().skip(1) {
            s += coefficient / (xx + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (xx + 0.5) * t.ln() - t + s.ln()
    }
}

/// Regularized lower incomplete gamma function P(a, x) = 纬(a,x)/螕(a).
pub(crate) fn regularized_lower_gamma(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x > a + 1.0 {
        // Use upper complement via continued fraction
        return 1.0 - regularized_upper_gamma_cf(a, x);
    }
    // Series expansion
    let log_prefix = -x + a * x.ln() - log_gamma_lanczos(a);
    let mut sum = 1.0_f64 / a;
    let mut term = 1.0_f64 / a;
    for n in 1..300 {
        term *= x / (a + f64::from(n));
        sum += term;
        if term.abs() < sum.abs() * 1e-14 {
            break;
        }
    }
    let result = sum * log_prefix.exp();
    result.clamp(0.0, 1.0)
}

/// Upper regularized gamma Q(a,x) via continued fraction (modified Lentz's method).
pub(crate) fn regularized_upper_gamma_cf(a: f64, x: f64) -> f64 {
    let log_prefix = -x + a * x.ln() - log_gamma_lanczos(a);
    let tiny = 1e-30_f64;

    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;

    for i in 1..300 {
        let an = -f64::from(i) * (f64::from(i) - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    let result = h * log_prefix.exp();
    result.clamp(0.0, 1.0)
}

/// Lanczos approximation of ln(Gamma(x)).
/// Matches Numerical Recipes 2nd edition `gammln()` exactly.
///
/// Currently used only in tests. The higher-precision [`log_gamma_lanczos`]
/// (g=7, 9 coefficients) is preferred in production code. This variant is
/// retained as a reference implementation for numeric parity checks.
#[cfg(test)]
fn ln_gamma(x: f64) -> f64 {
    let coefficients = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.001_208_650_973_866_179,
        -5.395_239_384_953_e-6,
    ];
    let tmp = x + 5.5;
    let mut ser = 1.000_000_000_190_015_f64;
    for (i, coeff) in coefficients.iter().enumerate() {
        ser += coeff / (x + 1.0 + i as f64);
    }
    -(tmp) + (x + 0.5) * tmp.ln() + (2.506_628_274_631_000_2_f64 * ser / x).ln()
}

/// Regularized incomplete beta function `I_x(a`, b) using continued fraction.
pub(crate) fn regularized_incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // Symmetry transform for better convergence
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - regularized_incomplete_beta(b, a, 1.0 - x);
    }
    let log_prefix = a * x.ln() + b * (1.0 - x).ln() - log_gamma_lanczos(a) - log_gamma_lanczos(b)
        + log_gamma_lanczos(a + b);
    let prefix = log_prefix.exp() / a;
    let mut c = 1.0_f64;
    let mut d = 1.0 / (1.0 - (a + b) * x / (a + 1.0));
    let mut f = d;
    for m in 1..200 {
        let m_f = f64::from(m);
        // Even step
        let a_even = m_f * (b - m_f) * x / ((a + 2.0 * m_f - 1.0) * (a + 2.0 * m_f));
        d = 1.0 / (1.0 + a_even * d);
        c = 1.0 + a_even / c;
        f *= c * d;
        // Odd step
        let a_odd = -(a + m_f) * (a + b + m_f) * x / ((a + 2.0 * m_f) * (a + 2.0 * m_f + 1.0));
        d = 1.0 / (1.0 + a_odd * d);
        c = 1.0 + a_odd / c;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < 1e-15 {
            break;
        }
    }
    prefix * f
}

// ---------------------------------------------------------------------------
// Logistic model diagnostics
// ---------------------------------------------------------------------------

/// Null model log-likelihood: `LL_0` = n1*ln(p1) + n0*ln(1-p1)
pub(crate) fn compute_null_log_likelihood(n_events: usize, n_total: usize) -> f64 {
    let p = n_events as f64 / n_total as f64;
    let p = p.clamp(1e-9, 1.0 - 1e-9);
    n_events as f64 * p.ln() + (n_total - n_events) as f64 * (1.0 - p).ln()
}

/// Nagelkerke pseudo-R虏 = (1 - exp(-2/n * (LL - LL0))) / (1 - exp(2/n * LL0))
pub(crate) fn compute_nagelkerke_r2(
    log_likelihood: f64,
    null_log_likelihood: f64,
    n: usize,
) -> f64 {
    let n_f = n as f64;
    let cox_snell = 1.0 - ((-2.0 / n_f) * (log_likelihood - null_log_likelihood)).exp();
    let max_r2 = 1.0 - ((2.0 / n_f) * null_log_likelihood).exp();
    if max_r2 > 0.0 {
        (cox_snell / max_r2).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// C-statistic (concordance / AUROC) for logistic regression.
pub(crate) fn compute_logistic_c_statistic(y: &[f64], predicted: &[f64]) -> f64 {
    if y.len() > 10_000 {
        eprintln!(
            "Warning: C-statistic computation is O(n\u{00b2}). n={} will require ~{} comparisons.",
            y.len(),
            (y.len() as u64).saturating_mul(y.len() as u64 - 1) / 2
        );
    }
    let mut concordant = 0u64;
    let mut discordant = 0u64;
    let mut tied = 0u64;
    for i in 0..y.len() {
        for j in (i + 1)..y.len() {
            if (y[i] - y[j]).abs() < 0.5 {
                continue;
            }
            let (event_pred, nonevent_pred) = if y[i] > y[j] {
                (predicted[i], predicted[j])
            } else {
                (predicted[j], predicted[i])
            };
            if event_pred > nonevent_pred {
                concordant += 1;
            } else if event_pred < nonevent_pred {
                discordant += 1;
            } else {
                tied += 1;
            }
        }
    }
    let total = concordant + discordant + tied;
    if total == 0 {
        return 0.5;
    }
    (concordant as f64 + 0.5 * tied as f64) / total as f64
}

// ---------------------------------------------------------------------------
// Cox concordance index (Harrell's C)
// ---------------------------------------------------------------------------

/// Cox concordance index using linear predictors.
pub(crate) fn compute_cox_concordance(observations: &[CoxObservation], beta: &[f64]) -> f64 {
    if observations.len() > 10_000 {
        eprintln!(
            "Warning: Cox concordance computation is O(n\u{00b2}). n={} may take a long time.",
            observations.len()
        );
    }
    let linear_predictors: Vec<f64> = observations.iter().map(|obs| dot(&obs.x, beta)).collect();
    let mut concordant = 0u64;
    let mut discordant = 0u64;
    let mut tied = 0u64;
    let n = observations.len();
    for i in 0..n {
        if !observations[i].event {
            continue;
        }
        for j in 0..n {
            if i == j {
                continue;
            }
            if observations[j].time > observations[i].time {
                if linear_predictors[i] > linear_predictors[j] {
                    concordant += 1;
                } else if linear_predictors[i] < linear_predictors[j] {
                    discordant += 1;
                } else {
                    tied += 1;
                }
            }
        }
    }
    let total = concordant + discordant + tied;
    if total == 0 {
        return 0.5;
    }
    (concordant as f64 + 0.5 * tied as f64) / total as f64
}

// ---------------------------------------------------------------------------
// Fisher information matrix for logistic regression
// ---------------------------------------------------------------------------

/// Compute Fisher information matrix X' W X (unweighted variant).
///
/// The weighted variant [`crate::logistic::fisher_information_weighted`] is
/// used in production logistic regression. This unweighted version is retained
/// for future diagnostic tools and unweighted model summaries.
// NOTE: dead_code allowed 鈥?planned for T2 statistical method expansion
#[allow(dead_code)]
pub(crate) fn fisher_information(x: &[Vec<f64>], probabilities: &[f64]) -> Vec<Vec<f64>> {
    let p = x[0].len();
    let mut info = vec![vec![0.0_f64; p]; p];
    for (i, x_row) in x.iter().enumerate() {
        let w = probabilities[i] * (1.0 - probabilities[i]);
        for j in 0..p {
            for k in j..p {
                let value = w * x_row[j] * x_row[k];
                info[j][k] += value;
                if j != k {
                    info[k][j] += value;
                }
            }
        }
    }
    info
}

// ---------------------------------------------------------------------------
// F and t distribution functions (used by linear regression)
// ---------------------------------------------------------------------------

/// Approximate p-value for F distribution using beta incomplete function approximation.
pub(crate) fn f_distribution_p_value(f: f64, df1: f64, df2: f64) -> f64 {
    if f <= 0.0 || !f.is_finite() {
        return 1.0;
    }
    // Use the relationship: F ~ Beta(df1/2, df2/2) via x = df1*f / (df1*f + df2)
    let x = df1 * f / (df1 * f + df2);
    // 1 - I_x(df1/2, df2/2) = I_{1-x}(df2/2, df1/2)
    let p = regularized_beta_incomplete(x, df1 / 2.0, df2 / 2.0);
    (1.0 - p).clamp(0.0, 1.0)
}

/// Regularized incomplete beta function `I_x(a, b)` 鈥?delegates to [`regularized_incomplete_beta`].
///
/// This is an alias with a different parameter order `(x, a, b)` vs `(a, b, x)`.
pub(crate) fn regularized_beta_incomplete(x: f64, a: f64, b: f64) -> f64 {
    regularized_incomplete_beta(a, b, x)
}

/// Two-sided p-value for the t distribution using the incomplete beta function.
pub(crate) fn t_distribution_p_value(t: f64, df: f64) -> f64 {
    if df <= 0.0 || !t.is_finite() {
        return 1.0;
    }
    let x = df / (df + t * t);
    let p = regularized_beta_incomplete(x, df / 2.0, 0.5);
    p.clamp(0.0, 1.0)
}

/// Critical value for the t distribution at a given two-sided alpha.
///
/// Finds `t_crit` such that `P(|T| > t_crit) = alpha` for `T ~ t(df)`.
/// Uses bisection on [`t_distribution_p_value`].
pub(crate) fn t_distribution_critical_value(alpha: f64, df: f64) -> f64 {
    if alpha <= 0.0 || alpha >= 1.0 {
        return f64::NAN;
    }
    if df <= 0.0 {
        return f64::NAN;
    }
    let target_p = alpha;
    let mut lo = 0.0;
    let mut hi = 50.0;
    // Expand hi until we bracket the root
    while t_distribution_p_value(hi, df) > target_p {
        hi *= 2.0;
        if hi > 1e5 {
            return hi;
        }
    }
    for _ in 0..120 {
        let mid = (lo + hi) / 2.0;
        let p = t_distribution_p_value(mid, df);
        if p < target_p {
            hi = mid;
        } else {
            lo = mid;
        }
        if hi - lo < 1e-12 {
            break;
        }
    }
    (lo + hi) / 2.0
}

/// Approximate critical value for t distribution at 97.5% (two-sided 95% CI).
pub(crate) fn t_critical_value_95(df: f64) -> f64 {
    if df <= 0.0 {
        return 1.96;
    }
    // For large df, use z approximation; for small df use a simple formula
    if df >= 120.0 {
        1.96
    } else if df >= 30.0 {
        1.96 + 2.0 / df
    } else {
        // Cornish-Fisher-like approximation for small df
        let g1 = 1.0 / (4.0 * df);
        let g2 = 1.0 / (32.0 * df * df);
        1.96 + 1.96 * g1 + 5.0 * 1.96 * g2 + (1.0 / df).sqrt() * 0.5
    }
}

/// Welch's t-test statistic and approximate degrees of freedom.
pub(crate) fn welch_t_statistic(a: &[f64], b: &[f64]) -> (f64, f64) {
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;
    let mean1 = a.iter().sum::<f64>() / n1;
    let mean2 = b.iter().sum::<f64>() / n2;
    let var1 = a.iter().map(|v| (v - mean1).powi(2)).sum::<f64>() / (n1 - 1.0);
    let var2 = b.iter().map(|v| (v - mean2).powi(2)).sum::<f64>() / (n2 - 1.0);
    let se = (var1 / n1 + var2 / n2).sqrt();
    if se <= 0.0 || !se.is_finite() {
        return (0.0, 1.0);
    }
    let t = (mean1 - mean2) / se;
    let numerator = (var1 / n1 + var2 / n2).powi(2);
    let denominator = (var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0);
    let df = if denominator > 0.0 {
        numerator / denominator
    } else {
        1.0
    };
    (t, df.max(1.0))
}

/// Two-sided p-value for Welch's t-test using normal approximation for large df.
pub(crate) fn welch_t_pvalue(t: f64, df: f64) -> f64 {
    if df > 100.0 {
        return 2.0 * (1.0 - normal_cdf(t.abs()));
    }
    let x = df / (df + t * t);
    regularized_incomplete_beta(df / 2.0, 0.5, x)
}

/// Kruskal-Wallis one-way analysis of variance by ranks.
pub(crate) fn kruskal_wallis_test(group_values: &[&[f64]]) -> Option<f64> {
    let k = group_values.len();
    if k < 2 {
        return None;
    }
    let mut pooled: Vec<(f64, usize)> = Vec::new();
    for (gi, values) in group_values.iter().enumerate() {
        for &v in *values {
            pooled.push((v, gi));
        }
    }
    let n = pooled.len();
    if n < 3 {
        return None;
    }
    pooled.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ranks = vec![0.0_f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && pooled[j].0 == pooled[i].0 {
            j += 1;
        }
        let avg_rank = (i + 1 + j) as f64 / 2.0;
        for rank in ranks.iter_mut().take(j).skip(i) {
            *rank = avg_rank;
        }
        i = j;
    }
    let mut group_n = vec![0usize; k];
    let mut group_rank_sum = vec![0.0_f64; k];
    for (idx, (_, gi)) in pooled.iter().enumerate() {
        group_n[*gi] += 1;
        group_rank_sum[*gi] += ranks[idx];
    }
    let n_f = n as f64;
    let mut h = 0.0_f64;
    for gi in 0..k {
        let ni = group_n[gi] as f64;
        if ni > 0.0 {
            let mean_rank = group_rank_sum[gi] / ni;
            h += ni * (mean_rank - f64::midpoint(n_f, 1.0)).powi(2);
        }
    }
    h *= 12.0 / (n_f * (n_f + 1.0));
    let df = (k - 1) as f64;
    Some(1.0 - chi_square_cdf(h, df))
}

/// Quantile from a sorted array using linear interpolation.
pub(crate) fn quantile_sorted(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    if values.len() == 1 {
        return values[0];
    }
    let n = values.len();
    let pos = quantile * (n - 1) as f64;
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;
    if lower == upper || upper >= n {
        return values[lower.min(n - 1)];
    }
    let frac = pos - lower as f64;
    values[lower] * (1.0 - frac) + values[upper] * frac
}

/// Poisson rate confidence interval per 1000 person-time.
pub(crate) fn poisson_rate_ci_per_1000(events: f64, person_time: f64) -> (f64, f64) {
    if person_time <= 0.0 || events < 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let rate = events / person_time * 1000.0;
    if events == 0.0 {
        return (
            0.0,
            -1000.0 * (1.0_f64 - 0.025_f64.powf(1.0)).ln() / person_time,
        );
    }
    let se_ln = 1.0 / events.sqrt();
    let ln_rate = rate.ln();
    let lower = (ln_rate - 1.96 * se_ln).exp();
    let upper = (ln_rate + 1.96 * se_ln).exp();
    (lower, upper)
}

// ---------------------------------------------------------------------------
// Re-export the CoxObservation type needed by compute_cox_concordance.
// It is defined in handlers but used via math. We accept this struct here
// so that the math module stays self-contained for concordance calculation.
// ---------------------------------------------------------------------------

/// A single observation for Cox proportional hazards model.
#[derive(Debug, Clone)]
pub(crate) struct CoxObservation {
    pub time: f64,
    pub event: bool,
    pub x: Vec<f64>,
    pub weight: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_cdf_known_values() {
        // R: pnorm(0) = 0.5 (A&S approximation has ~1e-7 precision)
        assert!(
            (normal_cdf(0.0) - 0.5).abs() < 1e-7,
            "pnorm(0): got {}",
            normal_cdf(0.0)
        );
        // R: pnorm(1.96) 鈮?0.97500210
        assert!((normal_cdf(1.96) - 0.975_002_10).abs() < 1e-6);
        // R: pnorm(-1.96) 鈮?0.02499790
        assert!((normal_cdf(-1.96) - 0.024_997_90).abs() < 1e-6);
        // R: pnorm(3.0) 鈮?0.9986501
        assert!((normal_cdf(3.0) - 0.998_650_1).abs() < 1e-5);
        // R: pnorm(-3.0) 鈮?0.001349898
        assert!((normal_cdf(-3.0) - 0.001_349_898).abs() < 1e-5);
    }

    #[test]
    fn chi_square_cdf_known_values() {
        // R: pchisq(3.841, df=1) 鈮?0.95 (critical value for p=0.05)
        let cdf = chi_square_cdf(3.841, 1.0);
        assert!(
            (cdf - 0.95).abs() < 0.005,
            "chi2 CDF at 3.841, df=1: got {cdf}"
        );
        // R: pchisq(5.991, df=2) 鈮?0.95
        let cdf2 = chi_square_cdf(5.991, 2.0);
        assert!(
            (cdf2 - 0.95).abs() < 0.005,
            "chi2 CDF at 5.991, df=2: got {cdf2}"
        );
        // R: pchisq(0.0, df=1) = 0.0
        assert!((chi_square_cdf(0.0, 1.0)).abs() < 1e-10);
    }

    #[test]
    fn fisher_exact_2x2_known_values() {
        // R: fisher.test(matrix(c(1, 9, 11, 3), nrow = 2))$p.value
        let p = fisher_exact_2x2(1, 9, 11, 3);
        assert!((p - 0.002_759_456).abs() < 1e-8, "p-value: got {p}");

        // Symmetric balanced table should not reject.
        let balanced = fisher_exact_2x2(5, 5, 5, 5);
        assert!(
            (balanced - 1.0).abs() < 1e-10,
            "balanced table p-value: got {balanced}"
        );
    }

    #[test]
    fn ln_gamma_known_values() {
        // 螕(1) = 1, ln(1) = 0
        assert!(ln_gamma(1.0).abs() < 1e-10);
        // 螕(2) = 1, ln(1) = 0
        assert!(ln_gamma(2.0).abs() < 1e-10);
        // 螕(5) = 24, ln(24) 鈮?3.178054
        assert!((ln_gamma(5.0) - 3.178_054).abs() < 1e-4);
        // 螕(0.5) = 鈭毾€ 鈮?1.7724539, ln(鈭毾€) 鈮?0.5723649
        assert!((ln_gamma(0.5) - 0.572_364_9).abs() < 1e-4);
    }

    #[test]
    fn welch_t_test_known_result() {
        // Two groups: a=[1,2,3,4,5], b=[3,4,5,6,7]
        // R: t.test(a, b) 鈫?t = -2.0, df = 8, p = 0.08058
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [3.0, 4.0, 5.0, 6.0, 7.0];
        let (t, df) = welch_t_statistic(&a, &b);
        assert!((t - (-2.0)).abs() < 0.01, "t-stat: got {t}");
        assert!((df - 8.0).abs() < 0.5, "df: got {df}");
        let p = welch_t_pvalue(t, df);
        assert!((p - 0.0806).abs() < 0.02, "p-value: got {p}");
    }

    #[test]
    fn nagelkerke_r2_bounds() {
        // With 50 events out of 100
        let null_ll = compute_null_log_likelihood(50, 100);
        // R: 50*log(0.5) + 50*log(0.5) 鈮?-69.31
        assert!((null_ll - (-69.31)).abs() < 0.1, "null LL: got {null_ll}");
        // Perfect model has LL = 0 鈫?R虏 should be close to 1
        let r2_perfect = compute_nagelkerke_r2(0.0, null_ll, 100);
        assert!(r2_perfect > 0.95, "perfect R2: got {r2_perfect}");
        // Null model 鈫?R虏 = 0
        let r2_null = compute_nagelkerke_r2(null_ll, null_ll, 100);
        assert!(r2_null.abs() < 1e-10, "null R2: got {r2_null}");
    }

    #[test]
    fn logistic_c_statistic_perfect_discrimination() {
        // Perfect discrimination: all events have higher predicted than non-events
        let y = vec![1.0, 1.0, 0.0, 0.0];
        let pred = vec![0.9, 0.8, 0.2, 0.1];
        let c = compute_logistic_c_statistic(&y, &pred);
        assert!((c - 1.0).abs() < 1e-10, "perfect c: got {c}");
        // Random discrimination
        let y2 = vec![1.0, 0.0, 1.0, 0.0];
        let pred2 = vec![0.5, 0.5, 0.5, 0.5];
        let c2 = compute_logistic_c_statistic(&y2, &pred2);
        assert!((c2 - 0.5).abs() < 1e-10, "random c: got {c2}");
    }

    #[test]
    fn cox_concordance_basic() {
        // Subject 1: event at t=1, x=[1.0] (high risk)
        // Subject 2: event at t=2, x=[0.5] (medium risk)
        // Subject 3: censored at t=3, x=[0.0] (low risk)
        let obs = vec![
            CoxObservation {
                time: 1.0,
                event: true,
                x: vec![1.0],
                weight: 1.0,
            },
            CoxObservation {
                time: 2.0,
                event: true,
                x: vec![0.5],
                weight: 1.0,
            },
            CoxObservation {
                time: 3.0,
                event: false,
                x: vec![0.0],
                weight: 1.0,
            },
        ];
        let beta = vec![1.0]; // positive 尾 鈫?higher x = higher risk
        let c = compute_cox_concordance(&obs, &beta);
        // All concordant 鈫?C = 1.0
        assert!((c - 1.0).abs() < 1e-10, "perfect cox c: got {c}");
    }

    #[test]
    fn linear_f_and_t_distribution_known_values() {
        // Test F-distribution p-value: F(10, 2, 100) should give very small p
        let p = f_distribution_p_value(10.0, 2.0, 100.0);
        assert!(p < 0.001, "F(10,2,100) p should be <0.001, got {p}");

        // F(0, 1, 1) 鈫?p=1
        let p2 = f_distribution_p_value(0.0, 1.0, 1.0);
        assert!((p2 - 1.0).abs() < 1e-10, "F(0) p should be 1.0, got {p2}");

        // Test t-distribution: large |t| 鈫?small p
        let p3 = t_distribution_p_value(5.0, 20.0);
        assert!(p3 < 0.001, "t(5,20) p should be <0.001, got {p3}");

        // t=0 鈫?p=1
        let p4 = t_distribution_p_value(0.0, 20.0);
        assert!((p4 - 1.0).abs() < 1e-8, "t(0) p should be 1.0, got {p4}");
    }
}

#[cfg(test)]
mod proptest_invariants {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// **Validates: Requirements 5**
        /// normal_cdf is monotonically increasing: x1 < x2 鈫?cdf(x1) 鈮?cdf(x2)
        #[test]
        fn normal_cdf_monotonicity(x1 in -6.0f64..6.0, x2 in -6.0f64..6.0) {
            let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
            prop_assert!(
                normal_cdf(lo) <= normal_cdf(hi),
                "normal_cdf({}) = {} > normal_cdf({}) = {}",
                lo, normal_cdf(lo), hi, normal_cdf(hi)
            );
        }

        /// **Validates: Requirements 5**
        /// normal_cdf symmetry: cdf(x) + cdf(-x) 鈮?1.0
        #[test]
        fn normal_cdf_symmetry(x in -6.0f64..6.0) {
            let sum = normal_cdf(x) + normal_cdf(-x);
            prop_assert!(
                (sum - 1.0).abs() < 1e-10,
                "normal_cdf({}) + normal_cdf({}) = {} (expected 鈮?1.0)",
                x, -x, sum
            );
        }

        /// **Validates: Requirements 5**
        /// normal_cdf range: 0 鈮?cdf(x) 鈮?1
        #[test]
        fn normal_cdf_range(x in -10.0f64..10.0) {
            let cdf = normal_cdf(x);
            prop_assert!(
                (0.0..=1.0).contains(&cdf),
                "normal_cdf({}) = {} out of [0, 1]",
                x, cdf
            );
        }

        /// **Validates: Requirements 5**
        /// chi_square_cdf is monotonically increasing for fixed df
        #[test]
        fn chi_square_cdf_monotonicity(
            x1 in 0.01f64..50.0,
            x2 in 0.01f64..50.0,
            df in 1.0f64..30.0
        ) {
            let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
            prop_assert!(
                chi_square_cdf(lo, df) <= chi_square_cdf(hi, df),
                "chi_square_cdf({}, {}) = {} > chi_square_cdf({}, {}) = {}",
                lo, df, chi_square_cdf(lo, df), hi, df, chi_square_cdf(hi, df)
            );
        }

        /// **Validates: Requirements 5**
        /// chi_square_cdf range: 0 鈮?result 鈮?1
        #[test]
        fn chi_square_cdf_range(x in 0.01f64..50.0, df in 1.0f64..30.0) {
            let cdf = chi_square_cdf(x, df);
            prop_assert!(
                (0.0..=1.0).contains(&cdf),
                "chi_square_cdf({}, {}) = {} out of [0, 1]",
                x, df, cdf
            );
        }

        /// **Validates: Requirements 5**
        /// log_gamma_lanczos recurrence: ln(螕(x+1)) = ln(x) + ln(螕(x))
        #[test]
        fn log_gamma_lanczos_recurrence(x in 0.5f64..100.0) {
            let lhs = log_gamma_lanczos(x + 1.0);
            let rhs = x.ln() + log_gamma_lanczos(x);
            prop_assert!(
                (lhs - rhs).abs() < 1e-8,
                "log_gamma_lanczos({} + 1) = {}, ln({}) + log_gamma_lanczos({}) = {}",
                x, lhs, x, x, rhs
            );
        }

        /// **Validates: Requirements 5**
        /// sigmoid range: 0 鈮?sigmoid(x) 鈮?1 (strict in exact math; f64 saturates at extremes)
        #[test]
        fn sigmoid_range(x in -500.0f64..500.0) {
            let s = sigmoid(x);
            prop_assert!(
                (0.0..=1.0).contains(&s),
                "sigmoid({}) = {} out of [0, 1]",
                x, s
            );
        }

        /// **Validates: Requirements 5**
        /// sigmoid symmetry: sigmoid(x) + sigmoid(-x) = 1
        #[test]
        fn sigmoid_symmetry(x in -500.0f64..500.0) {
            let sum = sigmoid(x) + sigmoid(-x);
            prop_assert!(
                (sum - 1.0).abs() < 1e-10,
                "sigmoid({}) + sigmoid({}) = {} (expected 鈮?1.0)",
                x, -x, sum
            );
        }

        /// **Validates: Requirements 5**
        /// quantile_sorted monotonicity: q1 < q2 鈫?quantile(q1) 鈮?quantile(q2)
        #[test]
        fn quantile_sorted_monotonicity(
            mut values in proptest::collection::vec(-1000.0f64..1000.0, 2..50),
            q1 in 0.0f64..1.0,
            q2 in 0.0f64..1.0
        ) {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (lo_q, hi_q) = if q1 <= q2 { (q1, q2) } else { (q2, q1) };
            prop_assert!(
                quantile_sorted(&values, lo_q) <= quantile_sorted(&values, hi_q),
                "quantile_sorted(_, {}) = {} > quantile_sorted(_, {}) = {}",
                lo_q, quantile_sorted(&values, lo_q), hi_q, quantile_sorted(&values, hi_q)
            );
        }
    }
}
