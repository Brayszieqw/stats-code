use super::{
    fs, path_matches, AnalysisCheckItem, AnalysisCheckLevel, BTreeSet, Path, PathBuf,
    ReportVerifyArgs, ReportVerifyResult, Value, ARTIFACT_SCHEMA_VERSION,
};

pub(crate) fn handle_report_verify(args: &ReportVerifyArgs) -> ReportVerifyResult {
    let artifacts_dir = args
        .artifacts
        .canonicalize()
        .unwrap_or_else(|_| args.artifacts.clone());
    let audit_dir = artifacts_dir.join("audit");
    let report_dir = artifacts_dir.join("report");
    let run_path = audit_dir.join("run.json");
    let evidence_path = audit_dir.join("evidence-index.json");
    let manifest_path = audit_dir.join("analysis_manifest.json");
    let report_path = report_dir.join("report.md");
    let mut items = Vec::new();
    let mut notes = Vec::new();

    verify_path_exists(
        &mut items,
        "artifacts_dir_found",
        "artifacts_dir_missing",
        &artifacts_dir,
        "artifacts directory",
    );
    verify_path_exists(
        &mut items,
        "audit_dir_found",
        "audit_dir_missing",
        &audit_dir,
        "audit directory",
    );
    verify_path_exists(
        &mut items,
        "report_markdown_found",
        "report_markdown_missing",
        &report_path,
        "report/report.md",
    );
    verify_path_exists(
        &mut items,
        "run_metadata_found",
        "run_metadata_missing",
        &run_path,
        "audit/run.json",
    );
    verify_path_exists(
        &mut items,
        "evidence_index_found",
        "evidence_index_missing",
        &evidence_path,
        "audit/evidence-index.json",
    );
    verify_path_exists(
        &mut items,
        "analysis_manifest_found",
        "analysis_manifest_missing",
        &manifest_path,
        "audit/analysis_manifest.json",
    );

    let run_json = read_report_verify_json(&run_path, &mut items, "run_metadata_json");
    let evidence_json = read_report_verify_json(&evidence_path, &mut items, "evidence_index_json");

    if let Some(run) = &run_json {
        verify_required_string(
            &mut items,
            run,
            &["schema_version"],
            "run_schema_version_present",
            "run_schema_version_missing",
            "audit/run.json schema_version",
        );
        verify_required_string(
            &mut items,
            run,
            &["stats_code_version"],
            "stats_code_version_present",
            "stats_code_version_missing",
            "audit/run.json stats_code_version",
        );
        verify_required_string(
            &mut items,
            run,
            &["analysis_fingerprint_fnv1a64"],
            "analysis_fingerprint_present",
            "analysis_fingerprint_missing",
            "audit/run.json analysis_fingerprint_fnv1a64",
        );
        verify_required_string(
            &mut items,
            run,
            &["data_fingerprint_fnv1a64"],
            "data_fingerprint_present",
            "data_fingerprint_missing",
            "audit/run.json data_fingerprint_fnv1a64",
        );
    }

    let mut accepted_count = 0usize;
    let mut rejected_count = 0usize;
    if let (Some(run), Some(evidence)) = (&run_json, &evidence_json) {
        verify_report_identity(run, evidence, &mut items);
        accepted_count = verify_accepted_report_artifacts(evidence, &artifacts_dir, &mut items);
        rejected_count = verify_rejected_report_artifacts(evidence, &mut items);
        if evidence
            .get("query")
            .and_then(|query| query.get("include_exploratory"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            push_verify_item(
                &mut items,
                AnalysisCheckLevel::Warning,
                "include_exploratory_enabled",
                "evidence-index query includes exploratory artifacts",
            );
        }
    }

    if accepted_count == 0 {
        push_verify_item(
            &mut items,
            AnalysisCheckLevel::Warning,
            "accepted_artifacts_empty",
            "no accepted artifacts were recorded in evidence-index.json",
        );
    }

    if rejected_count > 0 {
        notes.push(format!(
            "{rejected_count} rejected artifact(s) are recorded; inspect audit/evidence-index.json for reasons."
        ));
    }

    let error_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Error)
        .count();
    let warning_count = items
        .iter()
        .filter(|item| item.level == AnalysisCheckLevel::Warning)
        .count();
    let status = if error_count == 0 { "ok" } else { "error" }.to_string();

    ReportVerifyResult {
        status,
        artifacts_dir: artifacts_dir.display().to_string(),
        accepted_count,
        rejected_count,
        error_count,
        warning_count,
        items,
        notes,
    }
}

fn push_verify_item(
    items: &mut Vec<AnalysisCheckItem>,
    level: AnalysisCheckLevel,
    code: &str,
    message: impl Into<String>,
) {
    items.push(AnalysisCheckItem {
        level,
        code: code.to_string(),
        message: message.into(),
    });
}

fn verify_path_exists(
    items: &mut Vec<AnalysisCheckItem>,
    ok_code: &str,
    error_code: &str,
    path: &Path,
    label: &str,
) {
    if path.exists() {
        push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            ok_code,
            format!("{label} found at `{}`", path.display()),
        );
    } else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            error_code,
            format!("{label} is missing at `{}`", path.display()),
        );
    }
}

fn read_report_verify_json(
    path: &Path,
    items: &mut Vec<AnalysisCheckItem>,
    code_prefix: &str,
) -> Option<Value> {
    if !path.is_file() {
        return None;
    }
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<Value>(&contents) {
            Ok(value) => {
                push_verify_item(
                    items,
                    AnalysisCheckLevel::Ok,
                    &format!("{code_prefix}_valid"),
                    format!("{} parsed as JSON", path.display()),
                );
                Some(value)
            }
            Err(error) => {
                push_verify_item(
                    items,
                    AnalysisCheckLevel::Error,
                    &format!("{code_prefix}_invalid"),
                    format!("{} is not valid JSON: {error}", path.display()),
                );
                None
            }
        },
        Err(error) => {
            push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                &format!("{code_prefix}_unreadable"),
                format!("{} could not be read: {error}", path.display()),
            );
            None
        }
    }
}

fn verify_required_string(
    items: &mut Vec<AnalysisCheckItem>,
    value: &Value,
    keys: &[&str],
    ok_code: &str,
    error_code: &str,
    label: &str,
) {
    if string_at(value, keys).is_some() {
        push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            ok_code,
            format!("{label} is present"),
        );
    } else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            error_code,
            format!("{label} is missing"),
        );
    }
}

fn verify_report_identity(run: &Value, evidence: &Value, items: &mut Vec<AnalysisCheckItem>) {
    match (
        string_at(run, &["data_fingerprint_fnv1a64"]),
        string_at(evidence, &["query", "data_fingerprint_fnv1a64"]),
    ) {
        (Some(run_hash), Some(query_hash)) if run_hash == query_hash => push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            "data_fingerprint_matches",
            "run metadata and evidence-index data fingerprints match",
        ),
        (Some(run_hash), Some(query_hash)) => push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "data_fingerprint_mismatch",
            format!("run metadata data fingerprint `{run_hash}` differs from evidence-index `{query_hash}`"),
        ),
        _ => push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "data_fingerprint_missing_for_match",
            "run metadata or evidence-index data fingerprint is missing",
        ),
    }

    match (
        string_at(run, &["analysis_path"]),
        string_at(evidence, &["query", "analysis_path"]),
    ) {
        (Some(run_path), Some(query_path)) if path_matches(run_path, query_path) => {
            push_verify_item(
                items,
                AnalysisCheckLevel::Ok,
                "analysis_path_matches",
                "run metadata and evidence-index analysis paths match",
            );
        }
        (Some(run_path), Some(query_path)) => push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "analysis_path_mismatch",
            format!("run metadata analysis path `{run_path}` differs from evidence-index `{query_path}`"),
        ),
        _ => push_verify_item(
            items,
            AnalysisCheckLevel::Warning,
            "analysis_path_match_unavailable",
            "run metadata or evidence-index analysis path is missing",
        ),
    }

    match (
        string_at(run, &["data_path"]),
        string_at(evidence, &["query", "data_path"]),
    ) {
        (Some(run_path), Some(query_path)) if path_matches(run_path, query_path) => {
            push_verify_item(
                items,
                AnalysisCheckLevel::Ok,
                "data_path_matches",
                "run metadata and evidence-index data paths match",
            );
        }
        (Some(run_path), Some(query_path)) => push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "data_path_mismatch",
            format!(
                "run metadata data path `{run_path}` differs from evidence-index `{query_path}`"
            ),
        ),
        _ => push_verify_item(
            items,
            AnalysisCheckLevel::Warning,
            "data_path_match_unavailable",
            "run metadata or evidence-index data path is missing",
        ),
    }
}

fn verify_accepted_report_artifacts(
    evidence: &Value,
    artifacts_dir: &Path,
    items: &mut Vec<AnalysisCheckItem>,
) -> usize {
    let Some(accepted) = evidence.get("accepted_artifacts").and_then(Value::as_array) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "accepted_artifacts_missing",
            "evidence-index.json does not contain accepted_artifacts array",
        );
        return 0;
    };

    let include_exploratory = evidence
        .get("query")
        .and_then(|query| query.get("include_exploratory"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut seen_steps = BTreeSet::new();

    for (index, artifact) in accepted.iter().enumerate() {
        let label = format!("accepted_artifacts[{index}]");
        match string_at(artifact, &["status"]) {
            Some("accepted") => push_verify_item(
                items,
                AnalysisCheckLevel::Ok,
                "accepted_status_ok",
                format!("{label} status is accepted"),
            ),
            Some(status) => push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                "accepted_status_invalid",
                format!("{label} status is `{status}`, expected `accepted`"),
            ),
            None => push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                "accepted_status_missing",
                format!("{label} status is missing"),
            ),
        }

        verify_artifact_path_field(
            artifact,
            artifacts_dir,
            &["result_path"],
            &label,
            "artifact_result_found",
            "artifact_result_missing",
            items,
        );
        verify_artifact_path_field(
            artifact,
            artifacts_dir,
            &["context_path"],
            &label,
            "artifact_context_found",
            "artifact_context_missing",
            items,
        );
        verify_artifact_context_schema(artifact, artifacts_dir, &label, items);
        verify_accepted_artifact_diagnostics(artifact, artifacts_dir, &label, items);

        match artifact
            .get("matched_analysis_step_index")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(step_index) => {
                if seen_steps.insert(step_index) {
                    push_verify_item(
                        items,
                        AnalysisCheckLevel::Ok,
                        "analysis_step_match_present",
                        format!("{label} matched declared analysis step #{step_index}"),
                    );
                } else {
                    push_verify_item(
                        items,
                        AnalysisCheckLevel::Error,
                        "analysis_step_match_duplicate",
                        format!("{label} duplicates declared analysis step #{step_index}"),
                    );
                }
            }
            None => push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                "analysis_step_match_missing",
                format!("{label} does not record matched_analysis_step_index"),
            ),
        }

        match string_at(artifact, &["artifact", "role"]) {
            Some("declared") => push_verify_item(
                items,
                AnalysisCheckLevel::Ok,
                "artifact_role_declared",
                format!("{label} is declared evidence"),
            ),
            Some("exploratory") if include_exploratory => push_verify_item(
                items,
                AnalysisCheckLevel::Warning,
                "artifact_role_exploratory_included",
                format!("{label} is exploratory and was explicitly included"),
            ),
            Some("exploratory") => push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                "artifact_role_exploratory_unexpected",
                format!("{label} is exploratory but include_exploratory=false"),
            ),
            Some(role) => push_verify_item(
                items,
                AnalysisCheckLevel::Error,
                "artifact_role_invalid",
                format!("{label} has unexpected artifact role `{role}`"),
            ),
            None => push_verify_item(
                items,
                AnalysisCheckLevel::Warning,
                "artifact_role_missing",
                format!("{label} does not record artifact.role"),
            ),
        }

        match string_at(artifact, &["artifact", "status"]) {
            Some("produced" | "accepted") => push_verify_item(
                items,
                AnalysisCheckLevel::Ok,
                "artifact_status_valid",
                format!("{label} has a valid artifact.status"),
            ),
            Some(status) => push_verify_item(
                items,
                AnalysisCheckLevel::Warning,
                "artifact_status_unexpected",
                format!("{label} has unexpected artifact.status `{status}`"),
            ),
            None => push_verify_item(
                items,
                AnalysisCheckLevel::Warning,
                "artifact_status_missing",
                format!("{label} does not record artifact.status"),
            ),
        }
    }

    accepted.len()
}

fn verify_rejected_report_artifacts(evidence: &Value, items: &mut Vec<AnalysisCheckItem>) -> usize {
    let Some(rejected) = evidence.get("rejected_artifacts").and_then(Value::as_array) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "rejected_artifacts_missing",
            "evidence-index.json does not contain rejected_artifacts array",
        );
        return 0;
    };

    if rejected.is_empty() {
        push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            "rejected_artifacts_empty",
            "no rejected artifacts were recorded",
        );
    } else {
        for (index, artifact) in rejected.iter().enumerate() {
            let reason = string_at(artifact, &["reason"]).unwrap_or("no reason recorded");
            push_verify_item(
                items,
                AnalysisCheckLevel::Warning,
                "rejected_artifact_recorded",
                format!("rejected_artifacts[{index}] was rejected: {reason}"),
            );
        }
    }

    rejected.len()
}

fn verify_accepted_artifact_diagnostics(
    value: &Value,
    artifacts_dir: &Path,
    label: &str,
    items: &mut Vec<AnalysisCheckItem>,
) {
    let Some(raw) = string_at(value, &["result_path"]) else {
        return;
    };
    let path = resolve_artifact_reference(raw, artifacts_dir);
    if !path.is_file() {
        return;
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "accepted_artifact_result_unreadable",
            format!(
                "{label} result_path could not be read at `{}`",
                path.display()
            ),
        );
        return;
    };
    let Ok(result_value) = serde_json::from_str::<Value>(&contents) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "accepted_artifact_result_invalid_json",
            format!(
                "{label} result_path is not valid JSON at `{}`",
                path.display()
            ),
        );
        return;
    };
    if let Some(reason) = blocking_diagnostics_reason(&result_value) {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "accepted_artifact_blocking_diagnostics",
            format!("{label} was accepted but {reason}"),
        );
    } else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            "accepted_artifact_no_blocking_diagnostics",
            format!("{label} has no blocking diagnostics"),
        );
    }
}

fn verify_artifact_context_schema(
    value: &Value,
    artifacts_dir: &Path,
    label: &str,
    items: &mut Vec<AnalysisCheckItem>,
) {
    let Some(raw) = string_at(value, &["context_path"]) else {
        return;
    };
    let path = resolve_artifact_reference(raw, artifacts_dir);
    if !path.is_file() {
        return;
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "artifact_context_unreadable",
            format!(
                "{label} context_path could not be read at `{}`",
                path.display()
            ),
        );
        return;
    };
    let Ok(context_value) = serde_json::from_str::<Value>(&contents) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            "artifact_context_invalid_json",
            format!(
                "{label} context_path is not valid JSON at `{}`",
                path.display()
            ),
        );
        return;
    };
    match string_at(&context_value, &["artifact_schema_version"]) {
        Some(version) if version == ARTIFACT_SCHEMA_VERSION => push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            "artifact_schema_version_supported",
            format!("{label} context artifact_schema_version={ARTIFACT_SCHEMA_VERSION}"),
        ),
        Some(version) => push_verify_item(
            items,
            AnalysisCheckLevel::Warning,
            "artifact_schema_version_unexpected",
            format!("{label} context artifact_schema_version `{version}` is not supported"),
        ),
        None => push_verify_item(
            items,
            AnalysisCheckLevel::Warning,
            "artifact_schema_version_missing",
            format!("{label} context does not record artifact_schema_version"),
        ),
    }
}

fn verify_artifact_path_field(
    value: &Value,
    artifacts_dir: &Path,
    keys: &[&str],
    label: &str,
    ok_code: &str,
    error_code: &str,
    items: &mut Vec<AnalysisCheckItem>,
) {
    let field_name = keys.last().copied().unwrap_or("path");
    let Some(raw) = string_at(value, keys) else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            error_code,
            format!("{label} {field_name} is missing"),
        );
        return;
    };
    let path = resolve_artifact_reference(raw, artifacts_dir);
    if path.is_file() {
        push_verify_item(
            items,
            AnalysisCheckLevel::Ok,
            ok_code,
            format!("{label} {field_name} exists at `{}`", path.display()),
        );
    } else {
        push_verify_item(
            items,
            AnalysisCheckLevel::Error,
            error_code,
            format!("{label} {field_name} is missing at `{}`", path.display()),
        );
    }
}

fn resolve_artifact_reference(raw: &str, artifacts_dir: &Path) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        artifacts_dir.join(path)
    }
}

fn string_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    current.as_str().filter(|text| !text.trim().is_empty())
}

pub(super) fn blocking_diagnostics_reason(result_value: &Value) -> Option<String> {
    let codes = blocking_diagnostic_codes(result_value);
    if codes.is_empty() {
        None
    } else {
        Some(format!(
            "artifact has blocking diagnostics: {}",
            codes.join(", ")
        ))
    }
}

fn blocking_diagnostic_codes(result_value: &Value) -> Vec<String> {
    result_value
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    matches!(
                        string_at(diagnostic, &["severity"]),
                        Some("blocking" | "error")
                    )
                })
                .map(|diagnostic| {
                    string_at(diagnostic, &["code"])
                        .unwrap_or("blocking_diagnostic")
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Study context validation & template
// ---------------------------------------------------------------------------
