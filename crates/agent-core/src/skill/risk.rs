//! Risk signal detection for skill results (R7.3).
//!
//! Inspects a `SkillResult` payload and returns a set of [`RiskSignal`]s
//! indicating potential statistical concerns.

use crate::models::skill::RiskSignal;

/// Inspect a skill result payload and return all detected risk signals.
///
/// Detection rules (from Property 12 / Requirement 7.3):
/// - `payload.p_value > 0.05` ⇒ [`RiskSignal::PValueAboveAlpha`]
/// - `payload.vif` contains any value > 10.0 ⇒ [`RiskSignal::VifTooHigh`]
/// - `payload.power < 0.8` (or `payload.achieved_power < 0.8`) ⇒ [`RiskSignal::LowPower`]
/// - `payload.cox_ph_violated == true` (or `payload.ph_test.violated == true`) ⇒ [`RiskSignal::CoxPhAssumptionViolated`]
///
/// Fields not present or not exceeding thresholds produce no corresponding signal.
#[must_use] 
pub fn detect_risk_signals(payload: &serde_json::Value) -> Vec<RiskSignal> {
    let mut signals = Vec::new();

    // Check p_value > 0.05
    if let Some(p) = payload.get("p_value").and_then(serde_json::Value::as_f64) {
        if p > 0.05 {
            signals.push(RiskSignal::PValueAboveAlpha);
        }
    }

    // Check VIF > 10 (multicollinearity)
    if let Some(vif) = payload.get("vif").and_then(|v| v.as_object()) {
        if vif.values().any(|v| v.as_f64().is_some_and(|f| f > 10.0)) {
            signals.push(RiskSignal::VifTooHigh);
        }
    }

    // Check power < 0.8
    if let Some(power) = payload.get("power").and_then(serde_json::Value::as_f64) {
        if power < 0.8 {
            signals.push(RiskSignal::LowPower);
        }
    }
    // Also check achieved_power field (used by power analysis skill output)
    if let Some(power) = payload.get("achieved_power").and_then(serde_json::Value::as_f64) {
        if power < 0.8 && !signals.contains(&RiskSignal::LowPower) {
            signals.push(RiskSignal::LowPower);
        }
    }

    // Check Cox PH assumption violated
    if let Some(violated) = payload.get("cox_ph_violated").and_then(serde_json::Value::as_bool) {
        if violated {
            signals.push(RiskSignal::CoxPhAssumptionViolated);
        }
    }
    // Also check nested ph_test.violated (used by Cox regression output)
    if let Some(ph_test) = payload.get("ph_test").and_then(|v| v.as_object()) {
        if let Some(violated) = ph_test.get("violated").and_then(serde_json::Value::as_bool) {
            if violated && !signals.contains(&RiskSignal::CoxPhAssumptionViolated) {
                signals.push(RiskSignal::CoxPhAssumptionViolated);
            }
        }
    }

    signals
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_payload_no_signals() {
        let payload = json!({});
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_p_value_above_alpha() {
        let payload = json!({ "p_value": 0.12 });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::PValueAboveAlpha]);
    }

    #[test]
    fn test_p_value_at_boundary_no_signal() {
        let payload = json!({ "p_value": 0.05 });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_p_value_below_alpha_no_signal() {
        let payload = json!({ "p_value": 0.01 });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_vif_too_high() {
        let payload = json!({ "vif": { "age": 2.3, "bmi": 12.5 } });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::VifTooHigh]);
    }

    #[test]
    fn test_vif_at_boundary_no_signal() {
        let payload = json!({ "vif": { "age": 10.0, "bmi": 5.0 } });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_vif_all_below_no_signal() {
        let payload = json!({ "vif": { "age": 1.5, "bmi": 3.2 } });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_low_power() {
        let payload = json!({ "power": 0.6 });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::LowPower]);
    }

    #[test]
    fn test_power_at_boundary_no_signal() {
        let payload = json!({ "power": 0.8 });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_achieved_power_low() {
        let payload = json!({ "achieved_power": 0.55 });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::LowPower]);
    }

    #[test]
    fn test_both_power_fields_only_one_signal() {
        // When both power and achieved_power are low, only one LowPower signal
        let payload = json!({ "power": 0.5, "achieved_power": 0.4 });
        let signals = detect_risk_signals(&payload);
        assert_eq!(
            signals.iter().filter(|s| **s == RiskSignal::LowPower).count(),
            1
        );
    }

    #[test]
    fn test_cox_ph_violated_direct() {
        let payload = json!({ "cox_ph_violated": true });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::CoxPhAssumptionViolated]);
    }

    #[test]
    fn test_cox_ph_not_violated_no_signal() {
        let payload = json!({ "cox_ph_violated": false });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_cox_ph_violated_nested() {
        let payload = json!({ "ph_test": { "violated": true, "p_value": 0.02 } });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals, vec![RiskSignal::CoxPhAssumptionViolated]);
    }

    #[test]
    fn test_cox_ph_nested_not_violated_no_signal() {
        let payload = json!({ "ph_test": { "violated": false, "p_value": 0.15 } });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_both_cox_ph_fields_only_one_signal() {
        let payload = json!({
            "cox_ph_violated": true,
            "ph_test": { "violated": true }
        });
        let signals = detect_risk_signals(&payload);
        assert_eq!(
            signals
                .iter()
                .filter(|s| **s == RiskSignal::CoxPhAssumptionViolated)
                .count(),
            1
        );
    }

    #[test]
    fn test_multiple_signals_combined() {
        let payload = json!({
            "p_value": 0.15,
            "vif": { "x1": 15.0, "x2": 2.0 },
            "power": 0.5,
            "cox_ph_violated": true
        });
        let signals = detect_risk_signals(&payload);
        assert_eq!(signals.len(), 4);
        assert!(signals.contains(&RiskSignal::PValueAboveAlpha));
        assert!(signals.contains(&RiskSignal::VifTooHigh));
        assert!(signals.contains(&RiskSignal::LowPower));
        assert!(signals.contains(&RiskSignal::CoxPhAssumptionViolated));
    }

    #[test]
    fn test_no_signals_when_all_good() {
        let payload = json!({
            "p_value": 0.001,
            "vif": { "x1": 1.2, "x2": 2.5 },
            "power": 0.95,
            "cox_ph_violated": false
        });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_irrelevant_fields_ignored() {
        let payload = json!({
            "r_squared": 0.85,
            "aic": 123.4,
            "coefficients": [1.0, 2.0]
        });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_null_values_ignored() {
        let payload = json!({
            "p_value": null,
            "vif": null,
            "power": null,
            "cox_ph_violated": null
        });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }

    #[test]
    fn test_wrong_types_ignored() {
        let payload = json!({
            "p_value": "not a number",
            "vif": "not an object",
            "power": true,
            "cox_ph_violated": 42
        });
        let signals = detect_risk_signals(&payload);
        assert!(signals.is_empty());
    }
}
