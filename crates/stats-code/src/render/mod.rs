pub(crate) mod admin;
pub(crate) mod data;
pub(crate) mod report;
pub(crate) mod stats;
pub(crate) mod writer;

// Re-export all render functions so callers can use `crate::render::render_*`
pub(crate) use admin::{
    render_ai_ask_text, render_audit_explain_text, render_auth_doctor_text, render_auth_set_text,
    render_config_text, render_doctor_text, render_init_project_text, render_open_report_text,
};
pub(crate) use data::{
    render_inspect_text, render_power_text, render_rate_text, render_survival_km_text,
    render_tableone_text,
};
pub(crate) use report::{
    render_analysis_check_text, render_planned_text, render_report_build_text,
    render_report_verify_text, render_workflow_run_text,
};
pub(crate) use stats::{
    render_cox_text, render_diagnostic_roc_text, render_linear_text, render_logistic_text,
    render_stats_planned_text,
};

pub(crate) fn format_p_value(p: f64) -> String {
    if !p.is_finite() {
        return "NA".to_string();
    }
    if p < 0.001 {
        "<0.001".to_string()
    } else {
        format!("{p:.4}")
    }
}

pub(crate) fn format_optional_number(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |number| format!("{number:.4}"))
}

#[cfg(test)]
mod format_tests {
    use super::format_p_value;

    #[test]
    fn format_p_value_uses_threshold_for_tiny_values() {
        assert_eq!(format_p_value(0.0), "<0.001");
        assert_eq!(format_p_value(0.0009), "<0.001");
        assert_eq!(format_p_value(0.001), "0.0010");
        assert_eq!(format_p_value(0.8355440287990006), "0.8355");
    }
}

#[cfg(test)]
mod tests;
