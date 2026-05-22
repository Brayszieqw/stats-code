use crate::schema::PowerResult;

use super::common::{inverse_normal_cdf, z_critical, EPS};

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
    let event_prob = f64::midpoint(
        1.0 - (-lambda1 * (accrual / 2.0 + followup)).exp(),
        1.0 - (-lambda2 * (accrual / 2.0 + followup)).exp(),
    );
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
