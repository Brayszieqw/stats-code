use std::fmt::Write as _;

use crate::schema::{
    AnalysisCheckLevel, AnalysisCheckResult, PlannedCommandResult, ReportBuildResult,
    ReportVerifyResult, WorkflowRunResult,
};

use super::writer::TextReportWriter;

pub fn render_report_build_text(result: &ReportBuildResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Report Build");
    w.field("Status", &result.status);
    w.field("Analysis", &result.analysis_path);
    w.field("Output dir", &result.output_dir);

    let mut out = w.finish();
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
    let mut w = TextReportWriter::new();
    w.title("Report Verify");
    w.field("Status", &result.status);
    w.field("Artifacts", &result.artifacts_dir);
    w.field(
        "Summary",
        format!(
            "accepted={} rejected={} errors={} warnings={}",
            result.accepted_count, result.rejected_count, result.error_count, result.warning_count
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

pub fn render_analysis_check_text(result: &AnalysisCheckResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Analysis Check");
    w.field("Status", &result.status);
    w.field("Analysis", &result.analysis_path);
    w.field("Data path", &result.data_path);
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

fn analysis_check_level_label(level: AnalysisCheckLevel) -> &'static str {
    match level {
        AnalysisCheckLevel::Ok => "OK",
        AnalysisCheckLevel::Warning => "WARNING",
        AnalysisCheckLevel::Error => "ERROR",
    }
}

pub fn render_workflow_run_text(result: &WorkflowRunResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Workflow Run");
    w.field("Status", &result.status);
    w.field("Run ID", &result.run_id);
    w.field("Analysis", &result.analysis_path);
    w.field("Data path", &result.data_path);
    w.field("Artifacts", &result.artifacts_dir);
    w.field("Report", &result.report_output_dir);

    let mut out = w.finish();
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

pub fn render_planned_text(result: &PlannedCommandResult) -> String {
    let mut w = TextReportWriter::new();
    w.title("Plan");
    w.field("Status", &result.status);
    w.field("Command", &result.command);
    w.field("Data path", &result.data_path);
    w.field_opt("Analysis", result.analysis_path.as_deref());
    w.field_opt("Formula", result.formula.as_deref());

    let mut out = w.finish();
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
