//! Named distribution helpers for statistical methods.

#![allow(dead_code)]

use super::{
    chi_square_cdf, f_distribution_p_value, t_distribution_critical_value, t_distribution_p_value,
};

/// Two-sided upper-tail p-value for Student's t statistic.
pub(crate) fn student_t_two_sided(t: f64, df: f64) -> f64 {
    t_distribution_p_value(t, df)
}

/// Positive two-sided Student's t critical value for significance level alpha.
pub(crate) fn student_t_inv(alpha: f64, df: f64) -> f64 {
    t_distribution_critical_value(alpha, df)
}

/// Upper-tail p-value for an F statistic.
pub(crate) fn f_distribution_p(f: f64, df1: f64, df2: f64) -> f64 {
    f_distribution_p_value(f, df1, df2)
}

/// Upper-tail p-value for a chi-square statistic.
pub(crate) fn chi_square_p(x: f64, df: f64) -> f64 {
    if x <= 0.0 || df <= 0.0 {
        return 1.0;
    }
    (1.0 - chi_square_cdf(x, df)).clamp(0.0, 1.0)
}

/// Approximate upper-tail p-value for Tukey's studentized range statistic.
///
/// This deterministic approximation uses the relationship `q / sqrt(2) ~ t`
/// for each pairwise contrast and applies a Sidak-style max-tail adjustment
/// across `k * (k - 1) / 2` comparisons.
#[must_use]
pub fn studentized_range_p(q: f64, k: usize, df: f64) -> f64 {
    if !q.is_finite() || q <= 0.0 || k < 2 || df <= 0.0 {
        return 1.0;
    }
    let comparisons = (k * (k - 1) / 2) as f64;
    let pair_tail = student_t_two_sided(q / 2.0_f64.sqrt(), df).clamp(0.0, 1.0);
    (1.0 - (1.0 - pair_tail).powf(comparisons)).clamp(0.0, 1.0)
}

/// Approximate Lilliefors-corrected K-S p-value for normality diagnostics.
pub(crate) fn lilliefors_p(d: f64, n: usize) -> f64 {
    if !d.is_finite() || d <= 0.0 || n == 0 {
        return 1.0;
    }
    let n = n as f64;
    let z = (n.sqrt() - 0.01 + 0.85 / n.sqrt()) * d;
    if z < 0.302 {
        1.0
    } else if z < 1.18 {
        (1.093 - 7.01256 * z + 2.99587 * z * z).exp()
    } else {
        (1.0776 - 5.22323 * z).exp()
    }
    .clamp(0.0, 1.0)
}

/// Inverse standard normal CDF using Peter J. Acklam's rational approximation.
pub(crate) fn inverse_normal(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }

    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];

    let plow = 0.02425;
    let phigh = 1.0 - plow;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        return (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    if p > phigh {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        return -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    let q = p - 0.5;
    let r = q * q;
    (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
        / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::normal_cdf;

    #[test]
    fn named_distribution_helpers_match_existing_kernels() {
        assert!((student_t_two_sided(2.0, 20.0) - t_distribution_p_value(2.0, 20.0)).abs() < 1e-12);
        assert!(
            (f_distribution_p(3.0, 2.0, 20.0) - f_distribution_p_value(3.0, 2.0, 20.0)).abs()
                < 1e-12
        );
        assert!((chi_square_p(3.841, 1.0) - 0.05).abs() < 0.01);
    }

    #[test]
    fn inverse_normal_known_values() {
        assert!(inverse_normal(0.5).abs() < 1e-12);
        assert!((normal_cdf(inverse_normal(0.975)) - 0.975).abs() < 1e-6);
    }

    #[test]
    fn adjusted_p_values_are_probabilities() {
        assert!((0.0..=1.0).contains(&studentized_range_p(3.0, 4, 20.0)));
        assert!((0.0..=1.0).contains(&lilliefors_p(0.12, 30)));
    }
}
