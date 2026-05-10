use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use serde_json::Value;

use crate::cli::{AuditExplainArgs, OpenReportArgs};
use crate::helpers::stringify_error;
use crate::schema::{AuditExplainArtifact, AuditExplainResult, OpenReportResult};
pub(crate) fn handle_audit_explain(args: &AuditExplainArgs) -> Result<AuditExplainResult, String> {
    let artifacts_dir = args.artifacts.canonicalize().map_err(|error| {
        format!(
            "Cannot read artifacts directory `{}`: {error}",
            args.artifacts.display()
        )
    })?;
    let evidence_index_path = artifacts_dir.join("audit").join("evidence-index.json");
    let evidence_text = fs::read_to_string(&evidence_index_path).map_err(|error| {
        format!(
            "Cannot read evidence index `{}`: {error}",
            evidence_index_path.display()
        )
    })?;
    let evidence: Value = serde_json::from_str(&evidence_text).map_err(|error| {
        format!(
            "Cannot parse evidence index `{}` as JSON: {error}",
            evidence_index_path.display()
        )
    })?;

    let accepted_artifacts =
        audit_artifact_entries(evidence.get("accepted_artifacts").and_then(Value::as_array));
    let rejected_artifacts =
        audit_artifact_entries(evidence.get("rejected_artifacts").and_then(Value::as_array));
    let policy_exceptions =
        audit_policy_exceptions(evidence.get("policy_exceptions").and_then(Value::as_array));

    let mut notes = vec![
        "Audit explain is read-only; it summarizes evidence-index.json without modifying artifacts."
            .to_string(),
        "Accepted artifacts are the only evidence candidates for the formal report.".to_string(),
    ];
    if !rejected_artifacts.is_empty() {
        notes.push(
            "Rejected artifacts were recorded and should not be treated as confirmatory evidence."
                .to_string(),
        );
    }
    if !policy_exceptions.is_empty() {
        notes.push(
            "Policy exceptions indicate user-allowed unsupported survey/privacy boundaries."
                .to_string(),
        );
    }

    Ok(AuditExplainResult {
        status: "ok".to_string(),
        artifacts_dir: artifacts_dir.display().to_string(),
        evidence_index_path: evidence_index_path.display().to_string(),
        accepted_count: accepted_artifacts.len(),
        rejected_count: rejected_artifacts.len(),
        policy_exception_count: policy_exceptions.len(),
        accepted_artifacts,
        rejected_artifacts,
        policy_exceptions,
        notes,
    })
}

fn audit_artifact_entries(items: Option<&Vec<Value>>) -> Vec<AuditExplainArtifact> {
    items.map_or_else(Vec::new, |items| {
        items
            .iter()
            .map(|item| AuditExplainArtifact {
                command: json_string(item, "command").unwrap_or_else(|| "<unknown>".to_string()),
                status: json_string(item, "status").unwrap_or_else(|| "<unknown>".to_string()),
                report_decision: json_string(item, "report_decision"),
                analysis_step_index: item
                    .get("matched_analysis_step_index")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        item.get("artifact")
                            .and_then(|artifact| artifact.get("analysis_step_index"))
                            .and_then(Value::as_u64)
                    })
                    .and_then(|value| usize::try_from(value).ok()),
                reason: json_string(item, "reason").unwrap_or_else(|| "<no reason>".to_string()),
                result_path: json_string(item, "result_path"),
                context_path: json_string(item, "context_path"),
            })
            .collect()
    })
}

fn audit_policy_exceptions(items: Option<&Vec<Value>>) -> Vec<String> {
    items.map_or_else(Vec::new, |items| {
        items
            .iter()
            .map(|item| {
                if let Some(message) = json_string(item, "message") {
                    message
                } else if let Some(code) = json_string(item, "code") {
                    code
                } else {
                    item.to_string()
                }
            })
            .collect()
    })
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn handle_open_report(args: &OpenReportArgs) -> Result<OpenReportResult, String> {
    let artifacts_dir = args.artifacts.canonicalize().map_err(|error| {
        format!(
            "Cannot read artifacts directory `{}`: {error}",
            args.artifacts.display()
        )
    })?;
    let report_path = artifacts_dir.join("report").join("report.md");
    if !report_path.is_file() {
        return Err(format!(
            "Report markdown was not found at `{}`. Run `stats-code workflow run ...` or `stats-code report build ...` first.",
            report_path.display()
        ));
    }

    let mut notes = vec![
        "Open report targets the generated markdown report under report/report.md.".to_string(),
        "Run `stats-code report verify` before treating report values as formal evidence."
            .to_string(),
    ];
    let opened = if args.print_only {
        notes.push("--print-only was set; the report path was not opened.".to_string());
        false
    } else {
        open_path_with_platform(&report_path)?;
        notes.push("Report open request was sent to the operating system.".to_string());
        true
    };

    Ok(OpenReportResult {
        status: "ok".to_string(),
        artifacts_dir: artifacts_dir.display().to_string(),
        report_path: report_path.display().to_string(),
        opened,
        notes,
    })
}

fn open_path_with_platform(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        ProcessCommand::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
    }
    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open")
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ProcessCommand::new("xdg-open")
            .arg(path)
            .status()
            .map_err(stringify_error)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("open command exited with status {status}"))
                }
            })
    }
}
