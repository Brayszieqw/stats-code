use super::markdown::{
    declared_inspect_step_index, model_declared_step_index, rate_declared_step_index,
    tableone_declared_step_index,
};
use super::verify::blocking_diagnostics_reason;
use super::{
    extract_string_field, fs, json, path_matches, resolve_path_str_for_match, stringify_error,
    AnalysisSpec, ArtifactMetadata, ArtifactRole, ArtifactStatus, CoxResult, DiscoveredRunArtifact,
    InspectResult, LinearResult, LogisticResult, ModelKind, Path, RateResult,
    ReportArtifactDecision, ReportEvidence, ReportEvidenceQuery, RunArtifactContext,
    TableOneResult, Value,
};

pub(super) fn report_artifact_decision_json(decision: &ReportArtifactDecision) -> Value {
    json!({
        "command": &decision.command,
        "run_dir": &decision.run_dir,
        "result_path": &decision.result_path,
        "context_path": &decision.context_path,
        "status": decision.status,
        "report_decision": report_decision_label(decision.status),
        "reason": &decision.reason,
        "matched_by": &decision.matched_by,
        "matched_analysis_step_index": decision.matched_analysis_step_index,
        "artifact": &decision.artifact,
    })
}

fn report_decision_label(status: ArtifactStatus) -> &'static str {
    match status {
        ArtifactStatus::Accepted => "accepted",
        ArtifactStatus::Rejected => "rejected",
        ArtifactStatus::Produced => "undecided",
    }
}

struct RejectedArtifactDecisionParams<'a> {
    command: &'a str,
    run_dir: &'a Path,
    result_path: Option<&'a Path>,
    context_path: Option<&'a Path>,
    reason: &'a str,
    matched_by: Option<String>,
    matched_analysis_step_index: Option<usize>,
    artifact: Option<ArtifactMetadata>,
}

fn rejected_artifact_decision(
    params: RejectedArtifactDecisionParams<'_>,
) -> ReportArtifactDecision {
    ReportArtifactDecision {
        command: params.command.to_string(),
        run_dir: params.run_dir.display().to_string(),
        result_path: params.result_path.map(|path| path.display().to_string()),
        context_path: params.context_path.map(|path| path.display().to_string()),
        status: ArtifactStatus::Rejected,
        reason: params.reason.to_string(),
        matched_by: params.matched_by,
        matched_analysis_step_index: params.matched_analysis_step_index,
        artifact: params.artifact,
    }
}

fn accepted_artifact_decision(discovered: &DiscoveredRunArtifact) -> ReportArtifactDecision {
    ReportArtifactDecision {
        command: discovered.command.clone(),
        run_dir: discovered.run_dir.clone(),
        result_path: Some(discovered.result_path.clone()),
        context_path: discovered.context_path.clone(),
        status: ArtifactStatus::Accepted,
        reason: "matched current analysis/data identity and declared analysis step".to_string(),
        matched_by: Some(discovered.matched_by.clone()),
        matched_analysis_step_index: discovered.matched_analysis_step_index,
        artifact: discovered.artifact.clone(),
    }
}

pub(super) fn discover_report_evidence(
    source_dir: &Path,
    query: &ReportEvidenceQuery,
    spec: &AnalysisSpec,
) -> Result<ReportEvidence, String> {
    let mut evidence = ReportEvidence {
        source_dir: source_dir.to_path_buf(),
        ..ReportEvidence::default()
    };
    if !source_dir.exists() {
        evidence.notes.push(format!(
            "Artifacts directory `{}` does not exist.",
            source_dir.display()
        ));
        return Ok(evidence);
    }

    let mut entries = fs::read_dir(source_dir)
        .map_err(stringify_error)?
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if matches!(name, "audit" | "report" | "tables") {
            continue;
        }

        let command_path = path.join("command.json");
        let result_path = path.join("result.json");
        if !command_path.is_file() || !result_path.is_file() {
            continue;
        }
        let context_path = path.join("context.json");

        let command_value: Value =
            serde_json::from_str(&fs::read_to_string(&command_path).map_err(stringify_error)?)
                .map_err(stringify_error)?;
        let Some(command_name) = command_value
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            evidence.notes.push(format!(
                "Skipped `{}` because command.json did not contain a string `command` field.",
                path.display()
            ));
            continue;
        };
        if command_name == "report_build" {
            continue;
        }

        let result_contents = fs::read_to_string(&result_path).map_err(stringify_error)?;
        let result_value: Value =
            serde_json::from_str(&result_contents).map_err(stringify_error)?;
        let mut context = None;
        if context_path.is_file() {
            match serde_json::from_str::<RunArtifactContext>(
                &fs::read_to_string(&context_path).map_err(stringify_error)?,
            ) {
                Ok(value) => {
                    context = Some(value);
                }
                Err(error) => evidence.notes.push(format!(
                    "Ignored malformed context metadata in `{}`: {}.",
                    context_path.display(),
                    error
                )),
            }
        }
        let context_artifact = context.as_ref().and_then(|value| value.artifact.clone());
        let context_path_for_decision = context_path.is_file().then_some(context_path.as_path());
        if context_artifact
            .as_ref()
            .is_some_and(|artifact| artifact.role == ArtifactRole::Exploratory)
            && !query.include_exploratory
        {
            let reason = "exploratory artifact was not requested";
            let matched_analysis_step_index = context_artifact
                .as_ref()
                .and_then(|artifact| artifact.analysis_step_index);
            evidence.rejected_artifacts.push(rejected_artifact_decision(
                RejectedArtifactDecisionParams {
                    command: &command_name,
                    run_dir: &path,
                    result_path: Some(&result_path),
                    context_path: context_path_for_decision,
                    reason,
                    matched_by: None,
                    matched_analysis_step_index,
                    artifact: context_artifact,
                },
            ));
            evidence
                .notes
                .push(format!("Skipped `{}` because {reason}.", path.display()));
            continue;
        }

        let Some(matched_by) = match_report_artifact(
            query,
            &command_name,
            command_value.get("request"),
            &result_value,
            context.as_ref(),
        ) else {
            let reason = "artifact did not match the current analysis/data identity";
            let matched_analysis_step_index = context_artifact
                .as_ref()
                .and_then(|artifact| artifact.analysis_step_index);
            evidence.rejected_artifacts.push(rejected_artifact_decision(
                RejectedArtifactDecisionParams {
                    command: &command_name,
                    run_dir: &path,
                    result_path: Some(&result_path),
                    context_path: context_path_for_decision,
                    reason,
                    matched_by: None,
                    matched_analysis_step_index,
                    artifact: context_artifact,
                },
            ));
            evidence.notes.push(format!(
                "Skipped `{}` because it did not match the current analysis/data identity.",
                path.display()
            ));
            continue;
        };
        match command_name.as_str() {
            "inspect" => {
                if let Ok(value) = serde_json::from_value::<InspectResult>(result_value.clone()) {
                    if let Some(step_index) = declared_inspect_step_index(spec) {
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.inspect = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            "tableone" => {
                if let Ok(value) = serde_json::from_value::<TableOneResult>(result_value.clone()) {
                    if let Some(step_index) = tableone_declared_step_index(spec, &value) {
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.tableone = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            "rate" => {
                if let Ok(value) = serde_json::from_value::<RateResult>(result_value.clone()) {
                    if let Some(step_index) = rate_declared_step_index(spec, &value) {
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.rate = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            "model_logistic" => {
                if let Ok(value) = serde_json::from_value::<LogisticResult>(result_value.clone()) {
                    if let Some(step_index) = model_declared_step_index(
                        spec,
                        ModelKind::Logistic,
                        Some(&value.outcome),
                        None,
                        None,
                        &value.predictors,
                    ) {
                        if let Some(reason) = blocking_diagnostics_reason(&result_value) {
                            evidence.rejected_artifacts.push(rejected_artifact_decision(
                                RejectedArtifactDecisionParams {
                                    command: &command_name,
                                    run_dir: &path,
                                    result_path: Some(&result_path),
                                    context_path: context_path_for_decision,
                                    reason: &reason,
                                    matched_by: Some(matched_by),
                                    matched_analysis_step_index: Some(step_index),
                                    artifact: context_artifact,
                                },
                            ));
                            evidence
                                .notes
                                .push(format!("Skipped `{}` because {reason}.", path.display()));
                            continue;
                        }
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.logistic = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            "model_cox" => {
                if let Ok(value) = serde_json::from_value::<CoxResult>(result_value.clone()) {
                    if let Some(step_index) = model_declared_step_index(
                        spec,
                        ModelKind::Cox,
                        None,
                        Some(&value.time),
                        Some(&value.event),
                        &value.predictors,
                    ) {
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.cox = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            "model_linear" => {
                if let Ok(value) = serde_json::from_value::<LinearResult>(result_value) {
                    if let Some(step_index) = model_declared_step_index(
                        spec,
                        ModelKind::Linear,
                        Some(&value.outcome),
                        None,
                        None,
                        &value.predictors,
                    ) {
                        let discovered = DiscoveredRunArtifact {
                            command: command_name.clone(),
                            run_dir: path.display().to_string(),
                            result_path: result_path.display().to_string(),
                            context_path: context_path
                                .is_file()
                                .then(|| context_path.display().to_string()),
                            matched_by,
                            matched_analysis_step_index: Some(step_index),
                            artifact: context_artifact,
                        };
                        evidence.linear = Some(value);
                        evidence
                            .accepted_artifacts
                            .push(accepted_artifact_decision(&discovered));
                        evidence.discovered_runs.push(discovered);
                    } else {
                        let reason = "artifact does not match a declared analysis step";
                        evidence.rejected_artifacts.push(rejected_artifact_decision(
                            RejectedArtifactDecisionParams {
                                command: &command_name,
                                run_dir: &path,
                                result_path: Some(&result_path),
                                context_path: context_path_for_decision,
                                reason,
                                matched_by: Some(matched_by),
                                matched_analysis_step_index: None,
                                artifact: context_artifact,
                            },
                        ));
                        evidence.notes.push(format!(
                            "Skipped `{}` because it does not match a declared analysis step.",
                            path.display()
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    if evidence.discovered_runs.is_empty() {
        evidence.notes.push(format!(
            "No analysis result artifacts were discovered in `{}`.",
            source_dir.display()
        ));
    }
    Ok(evidence)
}

// ---------------------------------------------------------------------------
// Markdown builders from evidence
// ---------------------------------------------------------------------------

fn match_report_artifact(
    query: &ReportEvidenceQuery,
    command_name: &str,
    request: Option<&Value>,
    result: &Value,
    context: Option<&RunArtifactContext>,
) -> Option<String> {
    if let Some(context) = context {
        if context.command != command_name {
            return None;
        }
        if let (Some(artifact_fingerprint), Some(query_fingerprint)) = (
            context.data_fingerprint_fnv1a64.as_deref(),
            query.data_fingerprint_fnv1a64.as_deref(),
        ) {
            if artifact_fingerprint == query_fingerprint {
                return Some("data_fingerprint".to_string());
            }
        }
        if let Some(artifact_data_path) = context.data_path_resolved.as_deref() {
            if path_matches(artifact_data_path, &query.data_path_resolved) {
                let analysis_matches = context
                    .analysis_path_resolved
                    .as_deref()
                    .is_none_or(|path| path_matches(path, &query.analysis_path_resolved));
                if analysis_matches {
                    return Some(if context.analysis_path_resolved.is_some() {
                        "resolved_analysis_and_data_path".to_string()
                    } else {
                        "resolved_data_path".to_string()
                    });
                }
            }
        }
    }

    let legacy_analysis_path = extract_string_field(result, &["analysis_path"])
        .or_else(|| request.and_then(|value| extract_string_field(value, &["analysis"])));
    let legacy_data_path = extract_string_field(result, &["data_path"])
        .or_else(|| request.and_then(|value| extract_string_field(value, &["data", "data_path"])));
    let legacy_base_dir = context
        .and_then(|value| value.cwd.as_deref())
        .map(Path::new);

    legacy_data_path.and_then(|data_path| {
        let resolved_data_path = resolve_path_str_for_match(&data_path, legacy_base_dir);
        if !path_matches(&resolved_data_path, &query.data_path_resolved) {
            return None;
        }

        match legacy_analysis_path {
            Some(analysis_path) => {
                let resolved_analysis_path =
                    resolve_path_str_for_match(&analysis_path, legacy_base_dir);
                if path_matches(&resolved_analysis_path, &query.analysis_path_resolved) {
                    Some("legacy_result_paths".to_string())
                } else {
                    None
                }
            }
            None => Some("legacy_data_path".to_string()),
        }
    })
}
