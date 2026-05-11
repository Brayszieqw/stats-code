use crate::cli::{PowerOneProportionArgs, PowerTwoMeansArgs, PowerTwoProportionsArgs};
use crate::schema::PowerResult;

pub fn power_one_proportion(args: &PowerOneProportionArgs) -> Result<PowerResult, String> {
    validate_probability(args.proportion, "proportion")?;
    validate_probability_exclusive(args.alpha, "alpha")?;
    if !args.margin.is_finite() || args.margin <= 0.0 {
        return Err("margin must be positive and finite.".to_string());
    }
    let z_alpha = inverse_standard_normal_cdf(1.0 - args.alpha / 2.0);
    let n = ((z_alpha * z_alpha * args.proportion * (1.0 - args.proportion))
        / (args.margin * args.margin))
        .ceil() as usize;

    Ok(PowerResult {
        status: "ok".to_string(),
        method: "one_proportion_precision".to_string(),
        alpha: args.alpha,
        power: None,
        allocation_ratio: None,
        total_n: n,
        group1_n: None,
        group2_n: None,
        effect_size: Some(args.margin),
        notes: vec![
            "One-sample proportion precision uses Wald normal approximation.".to_string(),
            format!("Target half-width margin of error: {:.4}.", args.margin),
        ],
        warnings: proportion_edge_warnings(args.proportion),
    })
}

pub fn power_two_proportions(args: &PowerTwoProportionsArgs) -> Result<PowerResult, String> {
    validate_probability(args.p1, "p1")?;
    validate_probability(args.p2, "p2")?;
    validate_probability_exclusive(args.alpha, "alpha")?;
    validate_probability_exclusive(args.power, "power")?;
    validate_allocation(args.allocation_ratio)?;
    let diff = (args.p1 - args.p2).abs();
    if diff <= f64::EPSILON {
        return Err("p1 and p2 must differ to compute two-proportion sample size.".to_string());
    }

    let z_alpha = inverse_standard_normal_cdf(1.0 - args.alpha / 2.0);
    let z_power = inverse_standard_normal_cdf(args.power);
    let allocation = args.allocation_ratio;
    let pooled = (args.p1 + allocation * args.p2) / (1.0 + allocation);
    let variance_null = pooled * (1.0 - pooled) * (1.0 + 1.0 / allocation);
    let variance_alt = args.p1 * (1.0 - args.p1) + args.p2 * (1.0 - args.p2) / allocation;
    let n1 = ((z_alpha * variance_null.sqrt() + z_power * variance_alt.sqrt()).powi(2)
        / (diff * diff))
        .ceil() as usize;
    let n2 = (n1 as f64 * allocation).ceil() as usize;

    let mut warnings = proportion_edge_warnings(args.p1);
    warnings.extend(proportion_edge_warnings(args.p2));
    warnings.sort();
    warnings.dedup();

    Ok(PowerResult {
        status: "ok".to_string(),
        method: "two_independent_proportions".to_string(),
        alpha: args.alpha,
        power: Some(args.power),
        allocation_ratio: Some(allocation),
        total_n: n1 + n2,
        group1_n: Some(n1),
        group2_n: Some(n2),
        effect_size: Some(diff),
        notes: vec![
            "Two-proportion sample size uses normal approximation for a two-sided superiority test.".to_string(),
            "allocation_ratio is n2/n1.".to_string(),
        ],
        warnings,
    })
}

pub fn power_two_means(args: &PowerTwoMeansArgs) -> Result<PowerResult, String> {
    if !args.mean1.is_finite() || !args.mean2.is_finite() {
        return Err("mean1 and mean2 must be finite.".to_string());
    }
    if !args.sd.is_finite() || args.sd <= 0.0 {
        return Err("sd must be positive and finite.".to_string());
    }
    validate_probability_exclusive(args.alpha, "alpha")?;
    validate_probability_exclusive(args.power, "power")?;
    validate_allocation(args.allocation_ratio)?;
    let diff = (args.mean1 - args.mean2).abs();
    if diff <= f64::EPSILON {
        return Err("mean1 and mean2 must differ to compute two-mean sample size.".to_string());
    }

    let z_alpha = inverse_standard_normal_cdf(1.0 - args.alpha / 2.0);
    let z_power = inverse_standard_normal_cdf(args.power);
    let allocation = args.allocation_ratio;
    let n1 = ((z_alpha + z_power).powi(2) * args.sd * args.sd * (1.0 + 1.0 / allocation)
        / (diff * diff))
        .ceil() as usize;
    let n2 = (n1 as f64 * allocation).ceil() as usize;

    Ok(PowerResult {
        status: "ok".to_string(),
        method: "two_independent_means".to_string(),
        alpha: args.alpha,
        power: Some(args.power),
        allocation_ratio: Some(allocation),
        total_n: n1 + n2,
        group1_n: Some(n1),
        group2_n: Some(n2),
        effect_size: Some(diff / args.sd),
        notes: vec![
            "Two-mean sample size uses normal approximation with common standard deviation."
                .to_string(),
            "allocation_ratio is n2/n1.".to_string(),
        ],
        warnings: Vec::new(),
    })
}

fn validate_probability(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0 and 1."))
    }
}

fn validate_probability_exclusive(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 && value < 1.0 {
        Ok(())
    } else {
        Err(format!("{name} must be strictly between 0 and 1."))
    }
}

fn validate_allocation(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err("allocation_ratio must be positive and finite.".to_string())
    }
}

fn proportion_edge_warnings(proportion: f64) -> Vec<String> {
    if (0.05..=0.95).contains(&proportion) {
        Vec::new()
    } else {
        vec!["normal_approximation_may_be_unstable_for_extreme_proportions".to_string()]
    }
}

fn inverse_standard_normal_cdf(probability: f64) -> f64 {
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
    const LOW: f64 = 0.024_25;
    const HIGH: f64 = 1.0 - LOW;

    if probability <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if probability >= 1.0 {
        return f64::INFINITY;
    }
    if probability < LOW {
        let q = (-2.0 * probability.ln()).sqrt();
        return (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    }
    if probability <= HIGH {
        let q = probability - 0.5;
        let r = q * q;
        return (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0);
    }
    let q = (-2.0 * (1.0 - probability).ln()).sqrt();
    -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
        / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_proportion_precision_matches_standard_example() {
        let result = power_one_proportion(&PowerOneProportionArgs {
            proportion: 0.5,
            margin: 0.05,
            alpha: 0.05,
        })
        .expect("power");

        assert_eq!(result.total_n, 385);
    }

    #[test]
    fn two_proportions_returns_balanced_group_sizes() {
        let result = power_two_proportions(&PowerTwoProportionsArgs {
            p1: 0.3,
            p2: 0.2,
            power: 0.8,
            alpha: 0.05,
            allocation_ratio: 1.0,
        })
        .expect("power");

        assert_eq!(result.group1_n, result.group2_n);
        assert!(result.total_n > 500);
    }

    #[test]
    fn two_means_uses_standardized_difference() {
        let result = power_two_means(&PowerTwoMeansArgs {
            mean1: 10.0,
            mean2: 12.0,
            sd: 4.0,
            power: 0.8,
            alpha: 0.05,
            allocation_ratio: 1.0,
        })
        .expect("power");

        assert_eq!(result.effect_size, Some(0.5));
        assert!(result.total_n > 120);
    }
}
