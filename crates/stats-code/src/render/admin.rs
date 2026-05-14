use std::fmt::Write as _;

use crate::schema::{
    AiAskResult, AnalysisCheckLevel, AuditExplainArtifact, AuditExplainResult, AuthDoctorResult,
    AuthSetResult, ConfigResult, DoctorResult, InitProjectResult, OpenReportResult,
};

use super::writer::TextReportWriter;

fn analysis_check_level_label(level: AnalysisCheckLevel) -> &'static str {
    match level {
        AnalysisCheckLevel::Ok => "OK",
        AnalysisCheckLevel::Warning => "WARNING",
        AnalysisCheckLevel::Error => "ERROR",
    }
}

pub fn render_auth_set_text(result: &AuthSetResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Auth Set");
    w.field("Status", &result.status);
    w.field("Provider", &result.provider);
    w.field("Config path", &result.config_path);
    w.field("API key env", &result.api_key_env);
    w.field_opt("Base URL env", result.base_url_env.as_deref());
    if !result.notes.is_empty() {
        w.field("Notes", "");
        let buf = w.finish();
        let mut out = buf.trim_end().to_string();
        out.push('\n');
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
        return out;
    }
    w.finish()
}

pub fn render_auth_doctor_text(result: &AuthDoctorResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Auth Doctor");
    let _ = writeln!(out, "  {:<17}{}", "Status", result.status);
    let _ = writeln!(out, "  {:<17}{}", "Config path", result.config_path);
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

pub fn render_config_text(result: &ConfigResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Config");
    w.field("Status", &result.status);
    w.field("Action", &result.action);
    w.field("Config path", &result.config_path);
    w.field(
        "Default model",
        result.default_model.as_deref().unwrap_or("<none>"),
    );
    w.field(
        "Saved models",
        if result.saved_models.is_empty() {
            "<none>".to_string()
        } else {
            result.saved_models.join(", ")
        },
    );
    w.field("Message", &result.message);
    if !result.notes.is_empty() {
        let buf = w.finish();
        let mut out = buf;
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
        return out;
    }
    w.finish()
}

pub fn render_ai_ask_text(result: &AiAskResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("AI Ask");
    w.field("Status", &result.status);
    w.field("Provider", &result.provider);
    w.field("Credential", &result.credential_source);
    w.field("Model", &result.model);
    w.field(
        "Tokens",
        format!(
            "in={} out={} total={}",
            result.input_tokens, result.output_tokens, result.total_tokens
        ),
    );
    w.field_opt("Request ID", result.request_id.as_deref());
    w.field("Prompt", &result.prompt);
    let mut out = w.finish();
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

pub fn render_doctor_text(result: &DoctorResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Doctor");
    w.field("Status", &result.status);
    w.field("Version", &result.version);
    w.field("Current dir", &result.current_dir);
    w.field("Executable", &result.executable);
    w.field(
        "Summary",
        format!(
            "errors={} warnings={}",
            result.error_count, result.warning_count
        ),
    );
    let mut out = w.finish();
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

pub fn render_init_project_text(result: &InitProjectResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Init Project");
    w.field("Status", &result.status);
    w.field("Project", &result.project_dir);
    w.field("Analysis", &result.analysis_path);
    w.field("Data dir", &result.data_dir);
    let mut out = w.finish();
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

pub fn render_open_report_text(result: &OpenReportResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Open Report");
    w.field("Status", &result.status);
    w.field("Artifacts", &result.artifacts_dir);
    w.field("Report", &result.report_path);
    w.field("Opened", result.opened);
    if !result.notes.is_empty() {
        let mut out = w.finish();
        let _ = writeln!(out, "  Notes");
        for note in &result.notes {
            let _ = writeln!(out, "  - {note}");
        }
        return out;
    }
    w.finish()
}

pub fn render_audit_explain_text(result: &AuditExplainResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Audit Explain");
    w.field("Status", &result.status);
    w.field("Artifacts", &result.artifacts_dir);
    w.field("Evidence index", &result.evidence_index_path);
    w.field(
        "Summary",
        format!(
            "accepted={} rejected={} policy_exceptions={}",
            result.accepted_count, result.rejected_count, result.policy_exception_count
        ),
    );
    let mut out = w.finish();
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
