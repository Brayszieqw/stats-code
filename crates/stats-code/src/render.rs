use std::fmt::Write as _;
use std::path::Path;

use serde_json::{json, Value};

use crate::schema::{
    format_variable_kind, AiAskResult, AnalysisCheckLevel, AnalysisCheckResult, AnalysisKind,
    AnalysisSpec, AuditExplainArtifact, AuditExplainResult, AuthDoctorResult, AuthSetResult,
    ColumnInspection, ConfigResult, CoxResult, DoctorResult, InitProjectResult, InspectResult,
    LinearResult, LogisticResult, ModelKind, OpenReportResult, PlannedCommandResult, RateResult,
    ReportBuildResult, ReportVerifyResult, TableOneResult, VariableRole, WorkflowRunResult,
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

pub fn render_inspect_text(result: &InspectResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Inspect");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(out, "  Format           {:?}", result.format);
    if let Some(rows) = result.rows {
        let _ = writeln!(out, "  Rows             {rows}");
    }
    let _ = writeln!(out, "  Columns          {}", result.columns);
    let _ = writeln!(out, "  Variables");
    for ColumnInspection {
        name,
        inferred_kind,
        missing_count,
        distinct_count,
        sample_values,
        numeric_summary,
        warnings,
        ..
    } in &result.variables
    {
        let numeric_summary = numeric_summary
            .as_ref()
            .map(|summary| {
                format!(
                    " min={:.4} mean={:.4} max={:.4} zeroes={}",
                    summary.min, summary.mean, summary.max, summary.zero_count
                )
            })
            .unwrap_or_default();
        let warning_text = if warnings.is_empty() {
            String::new()
        } else {
            format!(" warnings={}", warnings.join("|"))
        };
        let _ = writeln!(
            out,
            "  - {} [{}] missing={} distinct={} sample={}{}{}",
            name,
            format_variable_kind(*inferred_kind),
            missing_count,
            distinct_count,
            if sample_values.is_empty() {
                "<none>".to_string()
            } else {
                sample_values.join(", ")
            },
            numeric_summary,
            warning_text
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_planned_text(result: &PlannedCommandResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Plan");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Command          {}", result.command);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    if let Some(formula) = &result.formula {
        let _ = writeln!(out, "  Formula          {formula}");
    }
    let _ = writeln!(out, "  Outputs");
    for output in &result.expected_outputs {
        let _ = writeln!(out, "  - {output}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_tableone_text(result: &TableOneResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Table 1");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  By               {}", result.by);
    let _ = writeln!(
        out,
        "  Groups           {}",
        if result.group_levels.is_empty() {
            "<none>".to_string()
        } else {
            result.group_levels.join(", ")
        }
    );
    let _ = writeln!(out, "  Rows");
    for row in &result.rows {
        let label = row.label.as_deref().unwrap_or(&row.variable);
        let row_name = row
            .level
            .as_ref()
            .map_or_else(|| label.to_string(), |level| format!("{label} = {level}"));
        let group_cells = row
            .groups
            .iter()
            .map(|group| format!("{}: {}", group.group, group.cell.display))
            .collect::<Vec<_>>()
            .join(" | ");
        let p_text = match (&row.test_name, row.p_value) {
            (Some(test), Some(p)) => format!(" p={} ({test})", format_p_value(p)),
            _ => String::new(),
        };
        let warnings = if row.warnings.is_empty() {
            String::new()
        } else {
            format!(" warnings={}", row.warnings.join("|"))
        };
        let _ = writeln!(
            out,
            "  - {} [{}] overall={}{}{}",
            row_name,
            format_variable_kind(row.kind),
            row.overall.display,
            p_text,
            warnings
        );
        if !group_cells.is_empty() {
            let _ = writeln!(out, "    {group_cells}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_rate_text(result: &RateResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Rate");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Event            {}", result.event);
    let _ = writeln!(out, "  Person-time      {}", result.person_time);
    let _ = writeln!(
        out,
        "  Strata           {}",
        if result.strata.is_empty() {
            "<overall>".to_string()
        } else {
            result.strata.join(", ")
        }
    );
    let _ = writeln!(out, "  Rows");
    for row in &result.rows {
        let _ = writeln!(
            out,
            "  - {} records={}/{} events={:.3} pt={:.3} rate={:.6} per_1000={:.3} ci95=[{:.3}, {:.3}]",
            row.stratum,
            row.included_records,
            row.total_records,
            row.events,
            row.person_time,
            row.rate,
            row.rate_per_1000,
            row.lower_ci_per_1000,
            row.upper_ci_per_1000
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_logistic_text(result: &LogisticResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Logistic Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Outcome          {}", result.outcome);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Outcome counts   events={} nonevents={}",
        result.n_events, result.n_nonevents
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} iterations={} logLik={:.4}",
        result.converged, result.iterations, result.log_likelihood
    );
    if let Some(null_ll) = result.null_log_likelihood {
        let _ = writeln!(out, "  Null logLik      {null_ll:.4}");
    }
    let _ = writeln!(out, "  Diagnostics");
    if let Some(r2) = result.pseudo_r2_nagelkerke {
        let _ = writeln!(out, "  - Nagelkerke R²  {r2:.4}");
    }
    if let Some(aic) = result.aic {
        let _ = writeln!(out, "  - AIC            {aic:.2}");
    }
    if let Some(bic) = result.bic {
        let _ = writeln!(out, "  - BIC            {bic:.2}");
    }
    if let Some(c) = result.c_statistic {
        let _ = writeln!(out, "  - C-statistic    {c:.4}");
    }
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} OR={:.4} CI95=[{:.4}, {:.4}] p={} beta={:.4} se={:.4}{}{}",
            coefficient.term,
            coefficient.odds_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            p_value,
            coefficient.beta,
            coefficient.standard_error,
            level,
            reference
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_cox_text(result: &CoxResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Cox Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Time             {}", result.time);
    let _ = writeln!(out, "  Event            {}", result.event);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Event counts     events={} censored={} tied_event_times={}",
        result.n_events, result.n_censored, result.tied_event_times
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} iterations={} logPartialLik={:.4}",
        result.converged, result.iterations, result.log_partial_likelihood
    );
    if let Some(c) = result.concordance {
        let _ = writeln!(out, "  Concordance      {c:.4}");
    }
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} HR={:.4} CI95=[{:.4}, {:.4}] p={} beta={:.4} se={:.4}{}{}",
            coefficient.term,
            coefficient.hazard_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            p_value,
            coefficient.beta,
            coefficient.standard_error,
            level,
            reference
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_linear_text(result: &LinearResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Linear Model");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    if let Some(path) = &result.analysis_path {
        let _ = writeln!(out, "  Analysis         {path}");
    }
    let _ = writeln!(out, "  Formula          {}", result.formula);
    let _ = writeln!(out, "  Outcome          {}", result.outcome);
    let _ = writeln!(
        out,
        "  Predictors       {}",
        if result.predictors.is_empty() {
            "<none>".to_string()
        } else {
            result.predictors.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Rows             total={} used={} excluded_missing={} excluded_invalid={}",
        result.n_total, result.n_used, result.n_excluded_missing, result.n_excluded_invalid
    );
    let _ = writeln!(
        out,
        "  Fit              converged={} R²={:.4} adj_R²={:.4} RSE={:.4}",
        result.converged, result.r_squared, result.adjusted_r_squared, result.residual_std_error
    );
    if let Some(f) = result.f_statistic {
        let p_text = result
            .f_p_value
            .map(|p| format!(" p={}", format_p_value(p)))
            .unwrap_or_default();
        let _ = writeln!(out, "  F-statistic      {f:.4}{p_text}");
    }
    let _ = writeln!(out, "  Diagnostics");
    if let Some(aic) = result.aic {
        let _ = writeln!(out, "  - AIC            {aic:.2}");
    }
    if let Some(bic) = result.bic {
        let _ = writeln!(out, "  - BIC            {bic:.2}");
    }
    let _ = writeln!(out, "  Coefficients");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let level = coefficient
            .level
            .as_ref()
            .map(|level| format!(" level={level}"))
            .unwrap_or_default();
        let reference = coefficient
            .reference
            .as_ref()
            .map(|reference| format!(" ref={reference}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} beta={:.4} se={:.4} t={:.4} p={} CI95=[{:.4}, {:.4}]{}{}",
            coefficient.term,
            coefficient.beta,
            coefficient.standard_error,
            coefficient.t_statistic,
            p_value,
            coefficient.ci_lower,
            coefficient.ci_upper,
            level,
            reference
        );
    }
    if !result.warnings.is_empty() {
        let _ = writeln!(out, "  Warnings");
        for warning in &result.warnings {
            let _ = writeln!(out, "  - {warning}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_p_value;

    #[test]
    fn format_p_value_uses_threshold_for_tiny_values() {
        assert_eq!(format_p_value(0.0), "<0.001");
        assert_eq!(format_p_value(0.0009), "<0.001");
        assert_eq!(format_p_value(0.001), "0.0010");
        assert_eq!(format_p_value(0.8355440287990006), "0.8355");
    }
}

pub fn render_report_build_text(result: &ReportBuildResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Report Build");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Output dir       {}", result.output_dir);
    let _ = writeln!(out, "  Files");
    for file in &result.written_files {
        let _ = writeln!(out, "  - {file}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_report_verify_text(result: &ReportVerifyResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Report Verify");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(
        out,
        "  Summary          accepted={} rejected={} errors={} warnings={}",
        result.accepted_count, result.rejected_count, result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_analysis_check_text(result: &AnalysisCheckResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Analysis Check");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(
        out,
        "  Summary          errors={} warnings={}",
        result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

fn analysis_check_level_label(level: AnalysisCheckLevel) -> &'static str {
    match level {
        AnalysisCheckLevel::Ok => "OK",
        AnalysisCheckLevel::Warning => "WARNING",
        AnalysisCheckLevel::Error => "ERROR",
    }
}

pub fn render_workflow_run_text(result: &WorkflowRunResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Workflow Run");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Run ID           {}", result.run_id);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data path        {}", result.data_path);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Report           {}", result.report_output_dir);
    let _ = writeln!(out, "  Steps");
    for step in &result.steps {
        let _ = writeln!(
            out,
            "  - #{} {} status={} artifact={}",
            step.step_index, step.command, step.status, step.artifact_dir
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_init_project_text(result: &InitProjectResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Init Project");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Project          {}", result.project_dir);
    let _ = writeln!(out, "  Analysis         {}", result.analysis_path);
    let _ = writeln!(out, "  Data dir         {}", result.data_dir);
    let _ = writeln!(out, "  Written files");
    for file in &result.written_files {
        let _ = writeln!(out, "  - {file}");
    }
    if !result.next_steps.is_empty() {
        let _ = writeln!(out, "  Next steps");
        for step in &result.next_steps {
            let _ = writeln!(out, "  - {step}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_doctor_text(result: &DoctorResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Doctor");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Version          {}", result.version);
    let _ = writeln!(out, "  Current dir      {}", result.current_dir);
    let _ = writeln!(out, "  Executable       {}", result.executable);
    let _ = writeln!(
        out,
        "  Summary          errors={} warnings={}",
        result.error_count, result.warning_count
    );
    let _ = writeln!(out, "  Checks");
    for item in &result.items {
        let _ = writeln!(
            out,
            "  - {} {}: {}",
            analysis_check_level_label(item.level),
            item.code,
            item.message
        );
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_audit_explain_text(result: &AuditExplainResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Audit Explain");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Evidence index   {}", result.evidence_index_path);
    let _ = writeln!(
        out,
        "  Summary          accepted={} rejected={} policy_exceptions={}",
        result.accepted_count, result.rejected_count, result.policy_exception_count
    );
    write_audit_artifact_group(&mut out, "Accepted artifacts", &result.accepted_artifacts);
    write_audit_artifact_group(&mut out, "Rejected artifacts", &result.rejected_artifacts);
    if !result.policy_exceptions.is_empty() {
        let _ = writeln!(out, "  Policy exceptions");
        for exception in &result.policy_exceptions {
            let _ = writeln!(out, "  - {exception}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_open_report_text(result: &OpenReportResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Open Report");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Artifacts        {}", result.artifacts_dir);
    let _ = writeln!(out, "  Report           {}", result.report_path);
    let _ = writeln!(out, "  Opened           {}", result.opened);
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

fn write_audit_artifact_group(out: &mut String, title: &str, artifacts: &[AuditExplainArtifact]) {
    let _ = writeln!(out, "  {title}");
    if artifacts.is_empty() {
        let _ = writeln!(out, "  - <none>");
        return;
    }
    for artifact in artifacts {
        let step = artifact
            .analysis_step_index
            .map(|index| format!(" step=#{index}"))
            .unwrap_or_default();
        let decision = artifact
            .report_decision
            .as_ref()
            .map(|value| format!(" decision={value}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  - {} status={}{}{} reason={}",
            artifact.command, artifact.status, decision, step, artifact.reason
        );
    }
}

pub fn render_auth_set_text(result: &AuthSetResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Auth Set");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Provider         {}", result.provider);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(out, "  API key env      {}", result.api_key_env);
    if let Some(base_url_env) = &result.base_url_env {
        let _ = writeln!(out, "  Base URL env     {base_url_env}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_auth_doctor_text(result: &AuthDoctorResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Auth Doctor");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(out, "  Providers");
    for provider in &result.providers {
        let _ = writeln!(
            out,
            "  - {} model={} source={} api_key_present={} base_url_present={}",
            provider.provider,
            provider.model_hint,
            provider.credential_source,
            provider.api_key_present,
            provider.base_url_present
        );
        let _ = writeln!(
            out,
            "    env={}{}",
            provider.api_key_env,
            provider
                .base_url_env
                .as_ref()
                .map(|value| format!(" base_url_env={value}"))
                .unwrap_or_default()
        );
        if let Some(base_url) = &provider.configured_base_url {
            let _ = writeln!(out, "    configured_base_url={base_url}");
        }
        for note in &provider.notes {
            let _ = writeln!(out, "    note={note}");
        }
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_ai_ask_text(result: &AiAskResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "AI Ask");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Provider         {}", result.provider);
    let _ = writeln!(out, "  Credential       {}", result.credential_source);
    let _ = writeln!(out, "  Model            {}", result.model);
    let _ = writeln!(
        out,
        "  Tokens           in={} out={} total={}",
        result.input_tokens, result.output_tokens, result.total_tokens
    );
    if let Some(request_id) = &result.request_id {
        let _ = writeln!(out, "  Request ID       {request_id}");
    }
    let _ = writeln!(out, "  Prompt           {}", result.prompt);
    let _ = writeln!(out, "  Response");
    for line in result.response_text.lines() {
        let _ = writeln!(out, "  {line}");
    }
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn render_config_text(result: &ConfigResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Config");
    let _ = writeln!(out, "  Status           {}", result.status);
    let _ = writeln!(out, "  Action           {}", result.action);
    let _ = writeln!(out, "  Config path      {}", result.config_path);
    let _ = writeln!(
        out,
        "  Default model    {}",
        result.default_model.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(
        out,
        "  Saved models     {}",
        if result.saved_models.is_empty() {
            "<none>".to_string()
        } else {
            result.saved_models.join(", ")
        }
    );
    let _ = writeln!(out, "  Message          {}", result.message);
    if !result.notes.is_empty() {
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

pub fn build_command_log(spec: &AnalysisSpec) -> Value {
    let commands = spec
        .analyses
        .iter()
        .map(|step| match step.kind {
            AnalysisKind::Inspect => {
                json!({ "command": "stats-code inspect", "status": "planned" })
            }
            AnalysisKind::TableOne => json!({
                "command": "stats-code tableone",
                "by": step.by,
                "status": "planned"
            }),
            AnalysisKind::Rate => json!({
                "command": "stats-code rate",
                "event": step.event,
                "person_time": step.person_time,
                "status": "planned"
            }),
            AnalysisKind::Model => json!({
                "command": "stats-code model",
                "model": step.model,
                "outcome": step.outcome,
                "time": step.time,
                "event": step.event,
                "predictors": step.predictors,
                "adjust": step.adjust,
                "status": "planned"
            }),
        })
        .collect::<Vec<_>>();
    Value::Array(commands)
}

pub fn build_analysis_manifest(
    spec: &AnalysisSpec,
    analysis_path: &Path,
    data_path: &Path,
    analysis_fingerprint: Option<&str>,
    data_fingerprint: Option<&str>,
) -> Value {
    let checklist = study_context_checklist(spec);
    json!({
        "schema_version": spec.schema_version.as_deref().unwrap_or("stats-code.v0"),
        "stats_code_version": env!("CARGO_PKG_VERSION"),
        "analysis_path": analysis_path.display().to_string(),
        "analysis_fingerprint_fnv1a64": analysis_fingerprint,
        "data_path": data_path.display().to_string(),
        "data_fingerprint_fnv1a64": data_fingerprint,
        "study": {
            "title": &spec.study.title,
            "design": &spec.study.design,
            "population": &spec.study.population,
        },
        "study_context": {
            "estimand": &spec.study_context.estimand,
            "exposure": &spec.study_context.exposure,
            "comparator": &spec.study_context.comparator,
            "outcome": &spec.study_context.outcome,
            "time_zero": &spec.study_context.time_zero,
            "follow_up": &spec.study_context.follow_up,
            "censoring": &spec.study_context.censoring,
            "missing_data_strategy": &spec.study_context.missing_data_strategy,
            "clustering": &spec.study_context.clustering,
            "sensitivity_analyses": &spec.study_context.sensitivity_analyses,
            "reporting_guideline": &spec.study_context.reporting_guideline,
        },
        "reporting": {
            "recommended_guideline": recommended_reporting_guideline(&spec.study.design),
            "declared_guideline": &spec.study_context.reporting_guideline,
            "summary": {
                "present": checklist.iter().filter(|item| item.status == "present").count(),
                "missing": checklist.iter().filter(|item| item.status == "missing").count(),
                "recommended": checklist.iter().filter(|item| item.status == "recommended").count(),
            },
            "checklist": checklist.into_iter().map(|item| json!({
                "field": item.field,
                "status": item.status,
                "value": item.value,
                "note": item.note,
            })).collect::<Vec<_>>(),
        }
    })
}

pub fn build_study_context_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Study Context");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Recommended reporting guideline: {}",
        recommended_reporting_guideline(&spec.study.design)
    );
    if let Some(guideline) = &spec.study_context.reporting_guideline {
        let _ = writeln!(out, "- Declared reporting guideline: {guideline}");
    }

    for item in study_context_checklist(spec) {
        let _ = writeln!(
            out,
            "- {}: {}{}",
            item.field,
            item.value.unwrap_or_else(|| format!("<{}>", item.status)),
            if item.note.is_empty() {
                String::new()
            } else {
                format!(" ({})", item.note)
            }
        );
    }
    out
}

pub fn build_reporting_checklist_markdown(spec: &AnalysisSpec) -> String {
    let checklist = study_context_checklist(spec);
    let present = checklist
        .iter()
        .filter(|item| item.status == "present")
        .count();
    let missing = checklist
        .iter()
        .filter(|item| item.status == "missing")
        .count();
    let recommended = checklist
        .iter()
        .filter(|item| item.status == "recommended")
        .count();
    let mut out = String::new();
    let _ = writeln!(out, "# Reporting Checklist");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Recommended guideline: {}",
        recommended_reporting_guideline(&spec.study.design)
    );
    let _ = writeln!(
        out,
        "- Declared guideline: {}",
        spec.study_context
            .reporting_guideline
            .as_deref()
            .unwrap_or("<missing>")
    );
    let _ = writeln!(
        out,
        "- Summary: present={present}, missing={missing}, recommended={recommended}"
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "| Item | Status | Value | Note |");
    let _ = writeln!(out, "| --- | --- | --- | --- |");
    for item in checklist {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            item.field,
            item.status,
            item.value.unwrap_or_else(|| "<none>".to_string()),
            item.note
        );
    }
    out
}

pub fn build_methods_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Methods");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Study: {}", spec.study.title);
    let _ = writeln!(out, "- Design: {}", spec.study.design);
    if let Some(population) = &spec.study.population {
        let _ = writeln!(out, "- Population: {population}");
    }
    for item in study_context_checklist(spec)
        .into_iter()
        .filter(|item| item.status == "present")
    {
        let _ = writeln!(out, "- {}: {}", item.field, item.value.unwrap_or_default());
    }
    let _ = writeln!(out, "- Data source: {}", spec.data.path.display());
    let _ = writeln!(out, "- Data format: {:?}", spec.data.format);
    if let Some(dictionary_path) = &spec.data.dictionary_path {
        let _ = writeln!(out, "- Variable dictionary: {}", dictionary_path.display());
    }
    if let Some(survey) = &spec.survey {
        let _ = writeln!(out, "- Survey design:");
        if let Some(weight) = &survey.weight {
            let _ = writeln!(out, "  - Weight: `{weight}`");
        }
        if let Some(strata) = &survey.strata {
            let _ = writeln!(out, "  - Strata: `{strata}`");
        }
        if let Some(cluster) = &survey.cluster {
            let _ = writeln!(out, "  - Cluster: `{cluster}`");
        }
        if let Some(estimator) = &survey.variance_estimator {
            let _ = writeln!(out, "  - Variance estimator: `{estimator}`");
        }
        let _ = writeln!(
            out,
            "  - Note: supported deterministic Rust engines apply survey weights to point estimates; complex-survey variance still requires explicit review."
        );
    }
    if let Some(privacy) = &spec.privacy {
        let _ = writeln!(
            out,
            "- Privacy controls: deidentify={}, direct_identifiers=[{}], quasi_identifiers=[{}]",
            privacy.deidentify,
            privacy.direct_identifiers.join(", "),
            privacy.quasi_identifiers.join(", ")
        );
        let _ = writeln!(
            out,
            "  - Note: report markdown applies small-cell suppression when configured; de-identification and identifier removal still require explicit review."
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Planned Analyses");
    for step in &spec.analyses {
        match step.kind {
            AnalysisKind::Inspect => {
                let _ = writeln!(
                    out,
                    "- Dataset inspection with missingness and coding checks."
                );
            }
            AnalysisKind::TableOne => {
                let _ = writeln!(
                    out,
                    "- Table 1 baseline summary stratified by `{}`.",
                    step.by.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::Rate => {
                let _ = writeln!(
                    out,
                    "- Rate analysis using event `{}` and person-time `{}`.",
                    step.event.as_deref().unwrap_or("<unspecified>"),
                    step.person_time.as_deref().unwrap_or("<unspecified>")
                );
            }
            AnalysisKind::Model => match step.model {
                Some(ModelKind::Logistic) => {
                    let _ = writeln!(
                        out,
                        "- Logistic regression for `{}` with predictors `{}`.",
                        step.outcome.as_deref().unwrap_or("<unspecified>"),
                        if step.predictors.is_empty() {
                            "<none>".to_string()
                        } else {
                            step.predictors.join(", ")
                        }
                    );
                }
                Some(ModelKind::Cox) => {
                    let _ = writeln!(
                        out,
                        "- Cox proportional hazards model with time `{}` and event `{}`.",
                        step.time.as_deref().unwrap_or("<unspecified>"),
                        step.event.as_deref().unwrap_or("<unspecified>")
                    );
                }
                Some(ModelKind::Linear) => {
                    let _ = writeln!(
                        out,
                        "- Linear regression (OLS) for `{}` with predictors `{}`.",
                        step.outcome.as_deref().unwrap_or("<unspecified>"),
                        if step.predictors.is_empty() {
                            "<none>".to_string()
                        } else {
                            step.predictors.join(", ")
                        }
                    );
                }
                None => {
                    let _ = writeln!(out, "- Generic model step declared without model type.");
                }
            },
        }
    }
    out
}

pub fn build_variables_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Variable Dictionary");
    let _ = writeln!(out);
    for variable in &spec.variables {
        let roles = if variable.roles.is_empty() {
            "none".to_string()
        } else {
            variable
                .roles
                .iter()
                .map(|role| format_variable_role(*role))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let levels = variable
            .coding
            .as_ref()
            .map(|coding| {
                if coding.levels.is_empty() {
                    String::new()
                } else {
                    format!(", levels=[{}]", coding.levels.join(", "))
                }
            })
            .unwrap_or_default();
        let missing = variable
            .missing
            .as_ref()
            .map(|missing| {
                format!(
                    ", missing_codes=[{}], missing_strategy={}",
                    missing.codes.join(", "),
                    missing.strategy.as_deref().unwrap_or("unspecified")
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "- `{}`: kind=`{}`, roles=`{}`{}{}{}",
            variable.name,
            format_variable_kind(variable.kind),
            roles,
            variable
                .label
                .as_ref()
                .map(|label| format!(", label=\"{label}\""))
                .unwrap_or_default(),
            levels,
            missing
        );
    }
    out
}

pub fn build_report_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Analysis Report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "This report was scaffolded from `analysis.yaml` for `{}`.",
        spec.study.title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Study Context");
    for item in study_context_checklist(spec)
        .into_iter()
        .filter(|item| item.status == "present")
    {
        let _ = writeln!(out, "- {}: {}", item.field, item.value.unwrap_or_default());
    }
    if spec
        .study_context
        .reporting_guideline
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        let _ = writeln!(
            out,
            "- Reporting guideline: <missing>; complete `reporting-checklist.md` before drafting manuscript text."
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Results Placeholders");
    let _ = writeln!(out, "- Table 1: baseline characteristics.");
    let _ = writeln!(
        out,
        "- Rate analysis: effect measures and confidence intervals."
    );
    let _ = writeln!(out, "- Regression models: adjusted effect estimates.");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Interpretation Notes");
    let _ = writeln!(
        out,
        "- Replace placeholder text only after CLI outputs are attached."
    );
    let _ = writeln!(out, "- Keep effect sizes, confidence intervals, and assumption checks linked to generated evidence files.");
    let _ = writeln!(out, "- Carry run metadata, data fingerprint, and software versions into manuscript-facing outputs.");
    out
}

pub fn build_assumptions_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Assumption Checks");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Missingness: {}.",
        spec.study_context
            .missing_data_strategy
            .as_deref()
            .unwrap_or("document complete-case or imputation strategy")
    );
    let _ = writeln!(
        out,
        "- Coding: verify reference levels and ordinal direction."
    );
    if let Some(censoring) = &spec.study_context.censoring {
        let _ = writeln!(
            out,
            "- Censoring: verify `{censoring}` is implemented consistently."
        );
    }
    if let Some(clustering) = &spec.study_context.clustering {
        let _ = writeln!(
            out,
            "- Clustering: confirm analytic handling for `{clustering}`."
        );
    }
    if spec.survey.is_some() {
        let _ = writeln!(out, "- Survey design: confirm weights were applied where supported and review strata, cluster, replicate-weight, and variance-estimator handling before inference.");
    }
    for step in &spec.analyses {
        if step.kind != AnalysisKind::Model {
            continue;
        }
        match step.model {
            Some(ModelKind::Logistic) => {
                let _ = writeln!(
                    out,
                    "- Logistic model: check separation, EPV, collinearity, calibration, ROC."
                );
            }
            Some(ModelKind::Cox) => {
                let _ = writeln!(out, "- Cox model: check proportional hazards, influential observations, functional form.");
            }
            Some(ModelKind::Linear) => {
                let _ = writeln!(out, "- Linear model: check normality of residuals, homoscedasticity, multicollinearity (VIF), influential observations.");
            }
            None => {}
        }
    }
    out
}

pub fn build_audit_trail_markdown(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let checklist = study_context_checklist(spec);
    let _ = writeln!(out, "# Audit Trail");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "- Schema version: {}",
        spec.schema_version.as_deref().unwrap_or("stats-code.v0")
    );
    let _ = writeln!(out, "- Study: {}", spec.study.title);
    let _ = writeln!(out, "- Data path: {}", spec.data.path.display());
    let _ = writeln!(out, "- Data format: {:?}", spec.data.format);
    let _ = writeln!(
        out,
        "- Declared analyses: {}",
        spec.analyses
            .iter()
            .map(|step| match step.kind {
                AnalysisKind::Inspect => "inspect".to_string(),
                AnalysisKind::TableOne => "tableone".to_string(),
                AnalysisKind::Rate => "rate".to_string(),
                AnalysisKind::Model => match step.model {
                    Some(ModelKind::Logistic) => "model_logistic".to_string(),
                    Some(ModelKind::Cox) => "model_cox".to_string(),
                    Some(ModelKind::Linear) => "model_linear".to_string(),
                    None => "model".to_string(),
                },
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        out,
        "- Reporting guideline: recommended={}, declared={}",
        recommended_reporting_guideline(&spec.study.design),
        spec.study_context
            .reporting_guideline
            .as_deref()
            .unwrap_or("<missing>")
    );
    let _ = writeln!(
        out,
        "- Study context completeness: present={}, missing={}, recommended={}",
        checklist
            .iter()
            .filter(|item| item.status == "present")
            .count(),
        checklist
            .iter()
            .filter(|item| item.status == "missing")
            .count(),
        checklist
            .iter()
            .filter(|item| item.status == "recommended")
            .count()
    );
    if let Some(audit) = &spec.audit {
        let _ = writeln!(
            out,
            "- Audit policy: save_commands={}, save_inputs={}, save_outputs={}, save_environment={}, save_decisions={}",
            audit.save_commands,
            audit.save_inputs,
            audit.save_outputs,
            audit.save_environment,
            audit.save_decisions
        );
    }
    if let Some(privacy) = &spec.privacy {
        let _ = writeln!(
            out,
            "- Privacy policy: deidentify={}, direct_identifiers=[{}], quasi_identifiers=[{}], small_cell_threshold={}",
            privacy.deidentify,
            privacy.direct_identifiers.join(", "),
            privacy.quasi_identifiers.join(", "),
            privacy
                .small_cell_threshold.map_or_else(|| "unspecified".to_string(), |value| value.to_string())
        );
    }
    let _ = writeln!(
        out,
        "- Execution policy: deterministic CLI first, agent layer optional and off by default."
    );
    let _ = writeln!(out, "- Safety policy: no network access or arbitrary command execution is assumed for statistical runs.");
    out
}

pub fn build_tables_readme(spec: &AnalysisSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Tables");
    let _ = writeln!(out);
    let _ = writeln!(out, "Expected table outputs for `{}`:", spec.study.title);
    for step in &spec.analyses {
        match step.kind {
            AnalysisKind::TableOne => {
                let _ = writeln!(out, "- `tableone.csv`");
            }
            AnalysisKind::Rate => {
                let _ = writeln!(out, "- `rate-summary.csv`");
            }
            AnalysisKind::Model => match step.model {
                Some(ModelKind::Logistic) => {
                    let _ = writeln!(out, "- `model-logistic-coefficients.csv`");
                }
                Some(ModelKind::Cox) => {
                    let _ = writeln!(out, "- `model-cox-coefficients.csv`");
                }
                Some(ModelKind::Linear) => {
                    let _ = writeln!(out, "- `model-linear-coefficients.csv`");
                }
                None => {}
            },
            AnalysisKind::Inspect => {}
        }
    }
    out
}

fn format_variable_role(role: VariableRole) -> &'static str {
    match role {
        VariableRole::Outcome => "outcome",
        VariableRole::Exposure => "exposure",
        VariableRole::Covariate => "covariate",
        VariableRole::Strata => "strata",
        VariableRole::Time => "time",
        VariableRole::Event => "event",
        VariableRole::Id => "id",
        VariableRole::Weight => "weight",
        VariableRole::Cluster => "cluster",
    }
}

#[derive(Clone)]
struct ChecklistItem {
    field: &'static str,
    status: &'static str,
    value: Option<String>,
    note: &'static str,
}

fn study_context_checklist(spec: &AnalysisSpec) -> Vec<ChecklistItem> {
    let needs_time_anchor = requires_time_anchor(spec);
    let needs_comparator = requires_comparator(spec);
    let needs_clustering = requires_clustering(spec);
    let context = &spec.study_context;
    vec![
        checklist_item(
            "estimand",
            context.estimand.clone(),
            true,
            "Target effect measure or quantity of interest.",
        ),
        checklist_item(
            "exposure",
            context.exposure.clone(),
            true,
            "Primary exposure or intervention.",
        ),
        checklist_item(
            "comparator",
            context.comparator.clone(),
            needs_comparator,
            "Comparator arm or reference strategy.",
        ),
        checklist_item(
            "outcome",
            context.outcome.clone(),
            true,
            "Outcome definition aligned with analysis outputs.",
        ),
        checklist_item(
            "time_zero",
            context.time_zero.clone(),
            needs_time_anchor,
            "Index date or start of follow-up.",
        ),
        checklist_item(
            "follow_up",
            context.follow_up.clone(),
            needs_time_anchor,
            "Follow-up window or stopping rule.",
        ),
        checklist_item(
            "censoring",
            context.censoring.clone(),
            needs_time_anchor,
            "Administrative or informative censoring rules.",
        ),
        checklist_item(
            "missing_data_strategy",
            context.missing_data_strategy.clone(),
            true,
            "Complete-case, imputation, or other handling plan.",
        ),
        checklist_item(
            "clustering",
            context.clustering.clone(),
            needs_clustering,
            "Clustered, repeated, or survey-aware analysis structure.",
        ),
        checklist_item(
            "sensitivity_analyses",
            context.sensitivity_analyses.clone(),
            false,
            "Planned robustness or bias analyses.",
        ),
        checklist_item(
            "reporting_guideline",
            context.reporting_guideline.clone(),
            true,
            "STROBE, RECORD, CONSORT, TRIPOD, or another declared guideline.",
        ),
    ]
}

fn checklist_item(
    field: &'static str,
    value: Option<String>,
    required: bool,
    note: &'static str,
) -> ChecklistItem {
    let normalized = value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    ChecklistItem {
        field,
        status: match (normalized.is_some(), required) {
            (true, _) => "present",
            (false, true) => "missing",
            (false, false) => "recommended",
        },
        value: normalized,
        note,
    }
}

fn recommended_reporting_guideline(design: &str) -> &'static str {
    let normalized = design.to_ascii_lowercase();
    if normalized.contains("trial") || normalized.contains("random") {
        "CONSORT"
    } else if normalized.contains("prediction")
        || normalized.contains("prognostic")
        || normalized.contains("diagnostic")
    {
        "TRIPOD"
    } else {
        "STROBE"
    }
}

fn requires_time_anchor(spec: &AnalysisSpec) -> bool {
    spec.analyses.iter().any(|step| {
        matches!(step.model, Some(ModelKind::Cox))
            || matches!(step.kind, AnalysisKind::Rate)
            || step.time.is_some()
            || step.event.is_some()
            || step.person_time.is_some()
    })
}

fn requires_comparator(spec: &AnalysisSpec) -> bool {
    spec.variables
        .iter()
        .any(|variable| variable.roles.contains(&VariableRole::Exposure))
        || spec.study.design.to_ascii_lowercase().contains("trial")
}

fn requires_clustering(spec: &AnalysisSpec) -> bool {
    spec.survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster))
}
