use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::cli::{ReportBuildArgs, ReportVerifyArgs};
use crate::helpers::{
    extract_string_field, fingerprint_file, path_matches, resolve_path_for_match,
    resolve_path_str_for_match, stringify_error, unix_timestamp_nanos,
};
use crate::render::{
    build_analysis_manifest, build_assumptions_markdown, build_audit_trail_markdown,
    build_command_log, build_methods_markdown, build_report_markdown,
    build_reporting_checklist_markdown, build_study_context_markdown, build_tables_readme,
    build_variables_markdown, format_p_value,
};
use crate::schema::{
    load_analysis_spec, validate_study_context, AnalysisCheckItem, AnalysisCheckLevel,
    AnalysisKind, AnalysisSpec, ArtifactMetadata, ArtifactRole, ArtifactStatus, CoxResult,
    InspectResult, LinearResult, LogisticResult, ModelKind, RateResult, ReportBuildResult,
    ReportVerifyResult, TableOneResult, VariableRole,
};

const ARTIFACT_SCHEMA_VERSION: &str = "1.0";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct ReportEvidence {
    pub source_dir: PathBuf,
    pub inspect: Option<InspectResult>,
    pub tableone: Option<TableOneResult>,
    pub rate: Option<RateResult>,
    pub logistic: Option<LogisticResult>,
    pub cox: Option<CoxResult>,
    pub linear: Option<LinearResult>,
    pub discovered_runs: Vec<DiscoveredRunArtifact>,
    pub accepted_artifacts: Vec<ReportArtifactDecision>,
    pub rejected_artifacts: Vec<ReportArtifactDecision>,
    pub notes: Vec<String>,
}

impl ReportEvidence {
    pub fn has_any_results(&self) -> bool {
        self.inspect.is_some()
            || self.tableone.is_some()
            || self.rate.is_some()
            || self.logistic.is_some()
            || self.cox.is_some()
            || self.linear.is_some()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReportEvidenceQuery {
    pub analysis_path_resolved: String,
    pub data_path_resolved: String,
    pub data_fingerprint_fnv1a64: Option<String>,
    pub include_exploratory: bool,
}

impl ReportEvidenceQuery {
    pub fn new(analysis_path: &Path, data_path: &Path, include_exploratory: bool) -> Self {
        Self {
            analysis_path_resolved: resolve_path_for_match(analysis_path),
            data_path_resolved: resolve_path_for_match(data_path),
            data_fingerprint_fnv1a64: fingerprint_file(data_path),
            include_exploratory,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredRunArtifact {
    pub command: String,
    pub run_dir: String,
    pub result_path: String,
    pub context_path: Option<String>,
    pub matched_by: String,
    pub matched_analysis_step_index: Option<usize>,
    pub artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReportArtifactDecision {
    pub command: String,
    pub run_dir: String,
    pub result_path: Option<String>,
    pub context_path: Option<String>,
    pub status: ArtifactStatus,
    pub reason: String,
    pub matched_by: Option<String>,
    pub matched_analysis_step_index: Option<usize>,
    pub artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunArtifactContext {
    #[serde(default)]
    pub artifact_schema_version: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub stats_code_version: Option<String>,
    pub command: String,
    #[serde(default)]
    pub analysis_path: Option<String>,
    #[serde(default)]
    pub analysis_path_resolved: Option<String>,
    #[serde(default)]
    pub analysis_fingerprint_fnv1a64: Option<String>,
    #[serde(default)]
    pub data_path: Option<String>,
    #[serde(default)]
    pub data_path_resolved: Option<String>,
    #[serde(default)]
    pub data_fingerprint_fnv1a64: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub generated_at_unix_nanos: Option<u128>,
    #[serde(default)]
    pub artifact: Option<ArtifactMetadata>,
}

// ---------------------------------------------------------------------------
// Public handler
// ---------------------------------------------------------------------------

pub(crate) fn handle_report_build(args: &ReportBuildArgs) -> Result<ReportBuildResult, String> {
    let analysis_path = args.analysis.canonicalize().map_err(stringify_error)?;
    let spec = load_analysis_spec(&analysis_path)?;
    ensure_study_context_ready(&analysis_path, &spec)?;
    let out_dir = args
        .out
        .clone()
        .or_else(|| {
            spec.report
                .as_ref()
                .map(|report| resolve_relative_to_analysis(&analysis_path, &report.out_dir))
        })
        .unwrap_or_else(|| PathBuf::from("stats-code-artifacts"));
    let artifacts_dir = args.artifacts.as_ref().map_or_else(
        || out_dir.clone(),
        |path| resolve_relative_to_analysis(&analysis_path, path),
    );
    let report_dir = out_dir.join("report");
    let tables_dir = out_dir.join("tables");
    let audit_dir = out_dir.join("audit");
    let data_path = resolve_relative_to_analysis(&analysis_path, &spec.data.path);
    let evidence_query =
        ReportEvidenceQuery::new(&analysis_path, &data_path, args.include_exploratory);
    fs::create_dir_all(&report_dir).map_err(stringify_error)?;
    fs::create_dir_all(&tables_dir).map_err(stringify_error)?;
    fs::create_dir_all(&audit_dir).map_err(stringify_error)?;

    let evidence = discover_report_evidence(&artifacts_dir, &evidence_query, &spec)?;

    let normalized_json = serde_json::to_string_pretty(&spec).map_err(stringify_error)?;
    let analysis_fingerprint = fingerprint_file(&analysis_path);
    let data_fingerprint = fingerprint_file(&data_path);
    let command_log = build_command_log(&spec);
    let analysis_manifest = serde_json::to_string_pretty(&build_analysis_manifest(
        &spec,
        &analysis_path,
        &data_path,
        analysis_fingerprint.as_deref(),
        data_fingerprint.as_deref(),
    ))
    .map_err(stringify_error)?;
    let methods = build_methods_markdown(&spec);
    let study_context = build_study_context_markdown(&spec);
    let variables = build_variables_markdown(&spec);
    let summary = if evidence.has_any_results() {
        build_report_markdown_from_evidence(&spec, &evidence)
    } else {
        build_report_markdown(&spec)
    };
    let assumptions = build_assumptions_markdown(&spec);
    let reporting_checklist = build_reporting_checklist_markdown(&spec);
    let audit_trail = build_audit_trail_markdown(&spec);
    let tables_readme = build_tables_readme_from_evidence(&spec, &evidence);
    let study_title = spec.study.title.clone();
    let study_design = spec.study.design.clone();
    let study_population = spec.study.population.clone();
    let run_metadata = serde_json::to_string_pretty(&json!({
        "schema_version": spec.schema_version.as_deref().unwrap_or("stats-code.v0"),
        "stats_code_version": env!("CARGO_PKG_VERSION"),
        "generated_at_unix_nanos": unix_timestamp_nanos(),
        "analysis_path": analysis_path.display().to_string(),
        "data_path": data_path.display().to_string(),
        "analysis_fingerprint_fnv1a64": analysis_fingerprint.as_deref().unwrap_or("unavailable"),
        "data_fingerprint_fnv1a64": data_fingerprint.as_deref().unwrap_or("unavailable"),
        "study": {
            "title": study_title,
            "design": study_design,
            "population": study_population,
        },
        "cwd": std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string()),
    }))
    .map_err(stringify_error)?;
    let evidence_index = serde_json::to_string_pretty(&json!({
        "artifacts_dir": artifacts_dir.display().to_string(),
        "query": {
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": evidence_query.analysis_path_resolved,
            "data_path": data_path.display().to_string(),
            "data_path_resolved": evidence_query.data_path_resolved,
            "data_fingerprint_fnv1a64": evidence_query.data_fingerprint_fnv1a64,
            "include_exploratory": evidence_query.include_exploratory,
        },
        "discovered_runs": evidence.discovered_runs.iter().map(|run| json!({
            "command": &run.command,
            "run_dir": &run.run_dir,
            "result_path": &run.result_path,
            "context_path": &run.context_path,
            "matched_by": &run.matched_by,
            "matched_analysis_step_index": run.matched_analysis_step_index,
            "artifact": &run.artifact,
        })).collect::<Vec<_>>(),
        "accepted_artifacts": evidence.accepted_artifacts.iter().map(report_artifact_decision_json).collect::<Vec<_>>(),
        "rejected_artifacts": evidence.rejected_artifacts.iter().map(report_artifact_decision_json).collect::<Vec<_>>(),
        "notes": evidence.notes.clone(),
    }))
    .map_err(stringify_error)?;

    let mut written_files = Vec::new();
    write_report_file(
        &audit_dir.join("analysis.normalized.json"),
        &normalized_json,
        &mut written_files,
    )?;
    write_report_file(
        &audit_dir.join("analysis_manifest.json"),
        &analysis_manifest,
        &mut written_files,
    )?;
    write_report_file(
        &audit_dir.join("commands.json"),
        &serde_json::to_string_pretty(&command_log).map_err(stringify_error)?,
        &mut written_files,
    )?;
    write_report_file(
        &audit_dir.join("run.json"),
        &run_metadata,
        &mut written_files,
    )?;
    write_report_file(
        &audit_dir.join("audit-trail.md"),
        &audit_trail,
        &mut written_files,
    )?;
    write_report_file(
        &audit_dir.join("evidence-index.json"),
        &evidence_index,
        &mut written_files,
    )?;
    write_report_file(&report_dir.join("methods.md"), &methods, &mut written_files)?;
    write_report_file(
        &report_dir.join("study-context.md"),
        &study_context,
        &mut written_files,
    )?;
    write_report_file(
        &report_dir.join("variables.md"),
        &variables,
        &mut written_files,
    )?;
    write_report_file(&report_dir.join("report.md"), &summary, &mut written_files)?;
    write_report_file(
        &report_dir.join("reporting-checklist.md"),
        &reporting_checklist,
        &mut written_files,
    )?;
    write_report_file(
        &report_dir.join("assumptions.md"),
        &assumptions,
        &mut written_files,
    )?;
    write_report_file(
        &tables_dir.join("README.md"),
        &tables_readme,
        &mut written_files,
    )?;
    if let Some(tableone) = &evidence.tableone {
        write_report_file(
            &tables_dir.join("tableone.md"),
            &build_tableone_markdown(tableone, small_cell_threshold(&spec)),
            &mut written_files,
        )?;
    }
    if let Some(rate) = &evidence.rate {
        write_report_file(
            &tables_dir.join("rate-summary.md"),
            &build_rate_markdown(rate),
            &mut written_files,
        )?;
    }
    if let Some(logistic) = &evidence.logistic {
        write_report_file(
            &tables_dir.join("model-logistic-summary.md"),
            &build_logistic_markdown(logistic),
            &mut written_files,
        )?;
    }
    if let Some(cox) = &evidence.cox {
        write_report_file(
            &tables_dir.join("model-cox-summary.md"),
            &build_cox_markdown(cox),
            &mut written_files,
        )?;
    }
    if let Some(linear) = &evidence.linear {
        write_report_file(
            &tables_dir.join("model-linear-summary.md"),
            &build_linear_markdown(linear),
            &mut written_files,
        )?;
    }

    Ok(ReportBuildResult {
        status: "ok".to_string(),
        analysis_path: analysis_path.display().to_string(),
        output_dir: out_dir.display().to_string(),
        written_files,
        notes: vec![
            "Report scaffold created from analysis.yaml.".to_string(),
            format!(
                "Consumed {} analysis result artifact(s) from `{}`.",
                evidence.discovered_runs.len(),
                artifacts_dir.display()
            ),
            if evidence.has_any_results() {
                "Observed results were merged into report and table markdown files.".to_string()
            } else {
                "No observed analysis results were found; report content remains template-based."
                    .to_string()
            },
        ],
    })
}

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

fn blocking_diagnostics_reason(result_value: &Value) -> Option<String> {
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

pub(crate) fn ensure_study_context_ready(
    analysis_path: &Path,
    spec: &AnalysisSpec,
) -> Result<(), String> {
    let issues = validate_study_context(spec);
    if issues.is_empty() {
        return Ok(());
    }

    Err(format!(
        "Analysis spec `{}` is not ready for analysis-driven commands because required `study_context` fields are missing:\n- {}\n\nSuggested template for `{}`:\n{}",
        analysis_path.display(),
        issues.join("\n- "),
        analysis_path.display(),
        build_study_context_template(spec),
    ))
}

fn build_study_context_template(spec: &AnalysisSpec) -> String {
    let outcome = first_variable_with_role(spec, VariableRole::Outcome)
        .or_else(|| first_variable_with_role(spec, VariableRole::Event))
        .or_else(|| {
            spec.analyses
                .iter()
                .find_map(|step| step.outcome.clone().or_else(|| step.event.clone()))
        })
        .unwrap_or_else(|| "<fill in outcome>".to_string());
    let exposure = first_variable_with_role(spec, VariableRole::Exposure)
        .unwrap_or_else(|| "<fill in exposure>".to_string());
    let clustering = spec
        .survey
        .as_ref()
        .and_then(|survey| survey.cluster.clone())
        .or_else(|| first_variable_with_role(spec, VariableRole::Cluster))
        .unwrap_or_else(|| "<if clustered, fill in cluster variable>".to_string());
    let guideline = crate::schema::recommended_reporting_guideline(&spec.study.design);

    let mut lines = vec!["study_context:".to_string()];
    lines.push(format!(
        "  estimand: {}",
        quote_yaml_placeholder("<fill in target effect measure>")
    ));
    lines.push(format!("  exposure: {}", quote_yaml_placeholder(&exposure)));
    lines.push(format!(
        "  comparator: {}",
        quote_yaml_placeholder("<fill in comparator>")
    ));
    lines.push(format!("  outcome: {}", quote_yaml_placeholder(&outcome)));
    if requires_time_anchor_template(spec) {
        lines.push(format!(
            "  time_zero: {}",
            quote_yaml_placeholder("<fill in index date>")
        ));
        lines.push(format!(
            "  follow_up: {}",
            quote_yaml_placeholder("<fill in follow-up window>")
        ));
        lines.push(format!(
            "  censoring: {}",
            quote_yaml_placeholder("<fill in censoring rule>")
        ));
    }
    lines.push(format!(
        "  missing_data_strategy: {}",
        quote_yaml_placeholder("<fill in missing-data handling>")
    ));
    if requires_clustering_template(spec) {
        lines.push(format!(
            "  clustering: {}",
            quote_yaml_placeholder(&clustering)
        ));
    }
    lines.push(format!(
        "  sensitivity_analyses: {}",
        quote_yaml_placeholder("<optional robustness analyses>")
    ));
    lines.push(format!(
        "  reporting_guideline: {}",
        quote_yaml_placeholder(guideline)
    ));
    lines.join("\n")
}

fn first_variable_with_role(spec: &AnalysisSpec, role: VariableRole) -> Option<String> {
    spec.variables
        .iter()
        .find(|variable| variable.roles.contains(&role))
        .map(|variable| variable.name.clone())
}

fn requires_time_anchor_template(spec: &AnalysisSpec) -> bool {
    spec.analyses.iter().any(|step| {
        step.time.is_some()
            || step.person_time.is_some()
            || matches!(step.kind, crate::schema::AnalysisKind::Rate)
            || matches!(step.model, Some(crate::schema::ModelKind::Cox))
    })
}

fn requires_clustering_template(spec: &AnalysisSpec) -> bool {
    spec.survey
        .as_ref()
        .and_then(|survey| survey.cluster.as_ref())
        .is_some()
        || spec
            .variables
            .iter()
            .any(|variable| variable.roles.contains(&VariableRole::Cluster))
}

fn quote_yaml_placeholder(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

// ---------------------------------------------------------------------------
// Data path resolution
// ---------------------------------------------------------------------------

pub(crate) fn resolve_data_path(
    explicit_data: Option<&PathBuf>,
    explicit_analysis: Option<&PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    if let Some(path) = explicit_data {
        return Ok((path.clone(), explicit_analysis.cloned()));
    }

    let analysis_path = if let Some(path) = explicit_analysis.cloned() {
        path
    } else {
        let default = PathBuf::from("analysis.yaml");
        if default.is_file() {
            default
        } else {
            return Err(
                "No `--data` was provided and `analysis.yaml` was not found in the current directory.".to_string(),
            );
        }
    };
    let spec = load_analysis_spec(&analysis_path)?;
    Ok((
        resolve_relative_to_analysis(&analysis_path, &spec.data.path),
        Some(analysis_path),
    ))
}

pub(crate) fn resolve_relative_to_analysis(analysis_path: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        analysis_path
            .parent()
            .map_or_else(|| path.to_path_buf(), |parent| parent.join(path))
    }
}

// ---------------------------------------------------------------------------
// Evidence discovery
// ---------------------------------------------------------------------------

fn report_artifact_decision_json(decision: &ReportArtifactDecision) -> Value {
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

fn discover_report_evidence(
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

const MODEL_SCIENTIFIC_NOTATION_ABS: f64 = 1.0e6;
const MODEL_SMALL_SCIENTIFIC_NOTATION_ABS: f64 = 1.0e-4;
const MODEL_UNSTABLE_INTERVAL_ABS: f64 = 1.0e100;

fn format_model_number(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        return "NA".to_string();
    }
    let abs = value.abs();
    if abs != 0.0
        && !(MODEL_SMALL_SCIENTIFIC_NOTATION_ABS..MODEL_SCIENTIFIC_NOTATION_ABS).contains(&abs)
    {
        format!("{value:.precision$e}")
    } else {
        format!("{value:.precision$}")
    }
}

fn has_unstable_model_interval(lower: f64, upper: f64) -> bool {
    !lower.is_finite()
        || !upper.is_finite()
        || lower.abs() >= MODEL_UNSTABLE_INTERVAL_ABS
        || upper.abs() >= MODEL_UNSTABLE_INTERVAL_ABS
        || lower > upper
}

fn format_model_ci(lower: f64, upper: f64, precision: usize) -> String {
    if has_unstable_model_interval(lower, upper) {
        "unstable".to_string()
    } else {
        format!(
            "[{}, {}]",
            format_model_number(lower, precision),
            format_model_number(upper, precision)
        )
    }
}

fn format_model_ci_phrase(lower: f64, upper: f64, precision: usize) -> String {
    if has_unstable_model_interval(lower, upper) {
        "(CI unstable)".to_string()
    } else {
        format_model_ci(lower, upper, precision)
    }
}

fn write_report_warnings(out: &mut String, label: &str, warnings: &[String]) {
    if !warnings.is_empty() {
        let _ = writeln!(out, "- {label} warnings: {}.", warnings.join(", "));
    }
}

fn write_model_table_warnings(out: &mut String, warnings: &[String]) {
    if !warnings.is_empty() {
        let _ = writeln!(out, "- Warnings: {}.", warnings.join(", "));
    }
}

fn build_report_markdown_from_evidence(spec: &AnalysisSpec, evidence: &ReportEvidence) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Analysis Report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "This report was scaffolded from `analysis.yaml` for `{}`.",
        spec.study.title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Evidence");
    let _ = writeln!(out, "- Artifacts source: {}", evidence.source_dir.display());
    let _ = writeln!(
        out,
        "- Discovered commands: {}",
        if evidence.discovered_runs.is_empty() {
            "<none>".to_string()
        } else {
            evidence
                .discovered_runs
                .iter()
                .map(|run| run.command.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    if let Some(inspect) = &evidence.inspect {
        let _ = writeln!(
            out,
            "- Dataset inspection: rows={}, columns={}, variables with missingness={}.",
            inspect.rows.unwrap_or(0),
            inspect.columns,
            inspect
                .variables
                .iter()
                .filter(|variable| variable.missing_count > 0)
                .count()
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Observed Results");
    if let Some(tableone) = &evidence.tableone {
        let _ = writeln!(
            out,
            "- Table 1 available for `{}` with {} group(s) and {} row(s).",
            tableone.by,
            tableone.group_levels.len(),
            tableone.rows.len()
        );
    } else if declares_tableone(spec) {
        let _ = writeln!(out, "- Table 1: no observed result found.");
    }
    if let Some(rate) = &evidence.rate {
        let top_rows = rate
            .rows
            .iter()
            .take(3)
            .map(|row| format!("{} = {:.2}/1000", row.stratum, row.rate_per_1000))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Rate summary for `{}` / `{}`: {}.",
            rate.event,
            rate.person_time,
            if top_rows.is_empty() {
                "<no rows>".to_string()
            } else {
                top_rows
            }
        );
    } else if declares_rate(spec) {
        let _ = writeln!(out, "- Rate analysis: no observed result found.");
    }
    if let Some(logistic) = &evidence.logistic {
        let top_terms = logistic
            .coefficients
            .iter()
            .filter(|coefficient| coefficient.term != "Intercept")
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} OR {} {}",
                    coefficient.term,
                    format_model_number(coefficient.odds_ratio, 2),
                    format_model_ci_phrase(coefficient.ci_lower, coefficient.ci_upper, 2)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Logistic model: outcome `{}`, n={}, events={}, {}.",
            logistic.outcome,
            logistic.n_used,
            logistic.n_events,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
        write_report_warnings(&mut out, "Logistic model", &logistic.warnings);
    } else if declares_model(spec, ModelKind::Logistic) {
        let _ = writeln!(out, "- Logistic model: no observed result found.");
    }
    if let Some(cox) = &evidence.cox {
        let top_terms = cox
            .coefficients
            .iter()
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} HR {} {}",
                    coefficient.term,
                    format_model_number(coefficient.hazard_ratio, 2),
                    format_model_ci_phrase(coefficient.ci_lower, coefficient.ci_upper, 2)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Cox model: time `{}`, event `{}`, n={}, events={}, {}.",
            cox.time,
            cox.event,
            cox.n_used,
            cox.n_events,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
        write_report_warnings(&mut out, "Cox model", &cox.warnings);
    } else if declares_model(spec, ModelKind::Cox) {
        let _ = writeln!(out, "- Cox model: no observed result found.");
    }
    if let Some(linear) = &evidence.linear {
        let top_terms = linear
            .coefficients
            .iter()
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} β {:.4} [{:.4}, {:.4}]",
                    coefficient.term, coefficient.beta, coefficient.ci_lower, coefficient.ci_upper
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(
            out,
            "- Linear model: outcome `{}`, n={}, R²={:.4}, {}.",
            linear.outcome,
            linear.n_used,
            linear.r_squared,
            if top_terms.is_empty() {
                "no coefficient summary".to_string()
            } else {
                top_terms
            }
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Interpretation Notes");
    let _ = writeln!(out, "- Generated tables in `tables/` are derived from stored command results, not free-text guesses.");
    if let Some(threshold) = small_cell_threshold(spec) {
        let _ = writeln!(
            out,
            "- Small-cell suppression was applied to report markdown tables for positive cells below {threshold}."
        );
    }
    let _ = writeln!(out, "- Carry effect sizes, confidence intervals, and warnings forward into manuscript-facing text.");
    let _ = writeln!(
        out,
        "- Re-run analyses if the data fingerprint or command parameters change."
    );
    out
}

fn build_tables_readme_from_evidence(spec: &AnalysisSpec, evidence: &ReportEvidence) -> String {
    let mut out = build_tables_readme(spec);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Observed table artifacts from `{}`:",
        evidence.source_dir.display()
    );
    if evidence.tableone.is_some() {
        let _ = writeln!(out, "- `tableone.md`");
    }
    if evidence.rate.is_some() {
        let _ = writeln!(out, "- `rate-summary.md`");
    }
    if evidence.logistic.is_some() {
        let _ = writeln!(out, "- `model-logistic-summary.md`");
    }
    if evidence.cox.is_some() {
        let _ = writeln!(out, "- `model-cox-summary.md`");
    }
    if evidence.linear.is_some() {
        let _ = writeln!(out, "- `model-linear-summary.md`");
    }
    if !evidence.has_any_results() {
        let _ = writeln!(out, "- No observed result files were discovered.");
    }
    out
}

fn declared_inspect_step_index(spec: &AnalysisSpec) -> Option<usize> {
    spec.analyses
        .iter()
        .position(|step| matches!(step.kind, AnalysisKind::Inspect))
}

fn declares_tableone(spec: &AnalysisSpec) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::TableOne))
}

fn declares_rate(spec: &AnalysisSpec) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::Rate))
}

fn declares_model(spec: &AnalysisSpec, model: ModelKind) -> bool {
    spec.analyses
        .iter()
        .any(|step| matches!(step.kind, AnalysisKind::Model) && step.model == Some(model))
}

fn tableone_declared_step_index(spec: &AnalysisSpec, result: &TableOneResult) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::TableOne)
            && optional_string_matches(step.by.as_deref(), &result.by)
    })
}

fn rate_declared_step_index(spec: &AnalysisSpec, result: &RateResult) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::Rate)
            && optional_string_matches(step.event.as_deref(), &result.event)
            && optional_string_matches(step.person_time.as_deref(), &result.person_time)
            && declared_list_matches(&step.strata, &result.strata)
    })
}

fn model_declared_step_index(
    spec: &AnalysisSpec,
    model: ModelKind,
    outcome: Option<&str>,
    time: Option<&str>,
    event: Option<&str>,
    predictors: &[String],
) -> Option<usize> {
    spec.analyses.iter().position(|step| {
        matches!(step.kind, AnalysisKind::Model)
            && step.model == Some(model)
            && optional_string_matches(step.outcome.as_deref(), outcome.unwrap_or_default())
            && optional_string_matches(step.time.as_deref(), time.unwrap_or_default())
            && optional_string_matches(step.event.as_deref(), event.unwrap_or_default())
            && declared_predictors_match(step, predictors)
    })
}

fn optional_string_matches(expected: Option<&str>, actual: &str) -> bool {
    expected.is_none_or(|expected| expected == actual)
}

fn declared_list_matches(expected: &[String], actual: &[String]) -> bool {
    expected.is_empty()
        || expected
            .iter()
            .all(|expected| actual.iter().any(|value| value == expected))
}

fn declared_predictors_match(
    step: &crate::schema::AnalysisStepSpec,
    actual_predictors: &[String],
) -> bool {
    step.predictors
        .iter()
        .chain(step.adjust.iter())
        .all(|expected| actual_predictors.iter().any(|value| value == expected))
}

fn small_cell_threshold(spec: &AnalysisSpec) -> Option<usize> {
    spec.privacy
        .as_ref()
        .and_then(|privacy| privacy.small_cell_threshold)
        .filter(|threshold| *threshold > 1)
}

fn build_tableone_markdown(result: &TableOneResult, small_cell_threshold: Option<usize>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Table 1");
    let _ = writeln!(out);
    if let Some(threshold) = small_cell_threshold {
        let _ = writeln!(
            out,
            "Positive cells with n < {threshold} are suppressed in this markdown table."
        );
        let _ = writeln!(out);
    }
    let _ = writeln!(
        out,
        "| Variable | Overall | {} |",
        result.group_levels.join(" | ")
    );
    let _ = writeln!(
        out,
        "| --- | --- | {} |",
        result
            .group_levels
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    );
    for row in &result.rows {
        let label = row.label.as_deref().unwrap_or(&row.variable);
        let name = row
            .level
            .as_ref()
            .map_or_else(|| label.to_string(), |level| format!("{label} = {level}"));
        let group_cells = result
            .group_levels
            .iter()
            .map(|group| {
                row.groups
                    .iter()
                    .find(|cell| &cell.group == group)
                    .map_or_else(
                        || "NA".to_string(),
                        |cell| format_tableone_cell(&cell.cell, small_cell_threshold),
                    )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(
            out,
            "| {name} | {} | {} |",
            format_tableone_cell(&row.overall, small_cell_threshold),
            group_cells
        );
    }
    out
}

fn format_tableone_cell(cell: &crate::schema::TableOneCell, threshold: Option<usize>) -> String {
    if let Some(threshold) = threshold {
        if is_small_positive_cell(cell, threshold) {
            return format!("suppressed (<{threshold})");
        }
    }
    cell.display.clone()
}

fn is_small_positive_cell(cell: &crate::schema::TableOneCell, threshold: usize) -> bool {
    let count = cell.count.unwrap_or(cell.n_non_missing);
    count > 0 && count < threshold
}

fn build_rate_markdown(result: &RateResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Rate Summary");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| Stratum | Events | Person-time | Rate | Rate per 1000 | 95% CI per 1000 |"
    );
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | --- |");
    for row in &result.rows {
        let _ = writeln!(
            out,
            "| {} | {:.3} | {:.3} | {:.6} | {:.3} | [{:.3}, {:.3}] |",
            row.stratum,
            row.events,
            row.person_time,
            row.rate,
            row.rate_per_1000,
            row.lower_ci_per_1000,
            row.upper_ci_per_1000
        );
    }
    out
}

fn build_logistic_markdown(result: &LogisticResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Logistic Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(out, "- Events: {}", result.n_events);
    write_model_table_warnings(&mut out, &result.warnings);
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | OR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            coefficient.term,
            format_model_number(coefficient.odds_ratio, 4),
            format_model_ci(coefficient.ci_lower, coefficient.ci_upper, 4),
            p_value
        );
    }
    out
}

fn build_cox_markdown(result: &CoxResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Cox Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(out, "- Events: {}", result.n_events);
    write_model_table_warnings(&mut out, &result.warnings);
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | HR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            coefficient.term,
            format_model_number(coefficient.hazard_ratio, 4),
            format_model_ci(coefficient.ci_lower, coefficient.ci_upper, 4),
            p_value
        );
    }
    out
}

fn build_linear_markdown(result: &LinearResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Linear Model Summary");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Formula: `{}`", result.formula);
    let _ = writeln!(out, "- Rows used: {}", result.n_used);
    let _ = writeln!(
        out,
        "- R²: {:.4}, Adjusted R²: {:.4}",
        result.r_squared, result.adjusted_r_squared
    );
    if let Some(f) = result.f_statistic {
        let p_text = result
            .f_p_value
            .map(|p| format!(", p={}", format_p_value(p)))
            .unwrap_or_default();
        let _ = writeln!(out, "- F-statistic: {f:.4}{p_text}");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | β | SE | t | p-value | 95% CI |");
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | --- |");
    for coefficient in &result.coefficients {
        let p_value = format_p_value(coefficient.p_value);
        let _ = writeln!(
            out,
            "| {} | {:.4} | {:.4} | {:.4} | {} | [{:.4}, {:.4}] |",
            coefficient.term,
            coefficient.beta,
            coefficient.standard_error,
            coefficient.t_statistic,
            p_value,
            coefficient.ci_lower,
            coefficient.ci_upper,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// File writing / artifact persistence
// ---------------------------------------------------------------------------

fn write_report_file(
    path: &Path,
    content: &str,
    written_files: &mut Vec<String>,
) -> Result<(), String> {
    fs::write(path, content).map_err(stringify_error)?;
    written_files.push(path.display().to_string());
    Ok(())
}

pub(crate) fn persist_run_artifacts_with_metadata(
    base_dir: &Path,
    command_name: &str,
    request: &Value,
    response: &Value,
    artifact: Option<&ArtifactMetadata>,
) -> Result<PathBuf, String> {
    let run_dir = base_dir.join(format!("{}-{}", command_name, unix_timestamp_nanos()));
    let artifact_id = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command_name)
        .to_string();
    fs::create_dir_all(&run_dir).map_err(stringify_error)?;
    fs::write(
        run_dir.join("command.json"),
        serde_json::to_string_pretty(&json!({
            "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
            "artifact_id": &artifact_id,
            "stats_code_version": env!("CARGO_PKG_VERSION"),
            "command": command_name,
            "request": request,
            "artifact": artifact,
        }))
        .map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    fs::write(
        run_dir.join("result.json"),
        serde_json::to_string_pretty(response).map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    fs::write(
        run_dir.join("context.json"),
        serde_json::to_string_pretty(&build_run_artifact_context(
            command_name,
            request,
            response,
            &artifact_id,
            artifact.cloned(),
        ))
        .map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    Ok(run_dir)
}

fn build_run_artifact_context(
    command_name: &str,
    request: &Value,
    response: &Value,
    artifact_id: &str,
    artifact: Option<ArtifactMetadata>,
) -> RunArtifactContext {
    let cwd = std::env::current_dir().ok();
    let analysis_path = extract_string_field(response, &["analysis_path"])
        .or_else(|| extract_string_field(request, &["analysis"]));
    let data_path = extract_string_field(response, &["data_path"])
        .or_else(|| extract_string_field(request, &["data", "data_path"]));

    RunArtifactContext {
        artifact_schema_version: Some(ARTIFACT_SCHEMA_VERSION.to_string()),
        artifact_id: Some(artifact_id.to_string()),
        stats_code_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        command: command_name.to_string(),
        analysis_path: analysis_path.clone(),
        analysis_path_resolved: analysis_path
            .as_deref()
            .map(|path| resolve_path_str_for_match(path, cwd.as_deref())),
        analysis_fingerprint_fnv1a64: analysis_path.as_deref().and_then(|path| {
            let resolved = resolve_path_str_for_match(path, cwd.as_deref());
            fingerprint_file(Path::new(&resolved))
        }),
        data_path: data_path.clone(),
        data_path_resolved: data_path
            .as_deref()
            .map(|path| resolve_path_str_for_match(path, cwd.as_deref())),
        data_fingerprint_fnv1a64: data_path.as_deref().and_then(|path| {
            let resolved = resolve_path_str_for_match(path, cwd.as_deref());
            fingerprint_file(Path::new(&resolved))
        }),
        cwd: cwd.map(|path| path.display().to_string()),
        generated_at_unix_nanos: Some(unix_timestamp_nanos()),
        artifact,
    }
}

// ---------------------------------------------------------------------------
// Artifact matching
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;

    use crate::cli::{
        Command, ReportBuildArgs, ReportCommand, ReportVerifyArgs, WorkflowCommand, WorkflowRunArgs,
    };
    use crate::helpers::{fingerprint_file, resolve_path_for_match};
    use crate::schema::{AnalysisSpec, Diagnostic, LogisticCoefficient, LogisticResult};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
    }

    fn test_cli(command: Command) -> crate::cli::Cli {
        crate::cli::Cli {
            json: false,
            artifacts_dir: None,
            session: None,
            model: "gpt".to_string(),
            system: None,
            max_tokens: None,
            engine: crate::bridge::Engine::Rust,
            command: Some(command),
        }
    }

    fn write_minimal_verified_report_artifacts(root: &std::path::Path) -> PathBuf {
        let out_dir = root.join("artifacts");
        let audit_dir = out_dir.join("audit");
        let report_dir = out_dir.join("report");
        let run_dir = out_dir.join("inspect-main");
        fs::create_dir_all(&audit_dir).expect("create audit dir");
        fs::create_dir_all(&report_dir).expect("create report dir");
        fs::create_dir_all(&run_dir).expect("create run dir");

        let analysis_path = root.join("analysis.yaml");
        let data_path = root.join("demo.csv");
        let result_path = run_dir.join("result.json");
        let context_path = run_dir.join("context.json");
        fs::write(&analysis_path, "schema_version: stats-code.v0\n").expect("write analysis");
        fs::write(&data_path, "disease\n1\n0\n").expect("write data");
        fs::write(&result_path, r#"{"status":"ok"}"#).expect("write result");
        fs::write(&context_path, r#"{"command":"inspect"}"#).expect("write context");
        fs::write(report_dir.join("report.md"), "# Report\n").expect("write report");
        fs::write(audit_dir.join("analysis_manifest.json"), "{}").expect("write manifest");
        fs::write(
            audit_dir.join("run.json"),
            serde_json::to_string_pretty(&json!({
                "schema_version": "stats-code.v0",
                "stats_code_version": "0.1.0",
                "analysis_path": analysis_path.display().to_string(),
                "data_path": data_path.display().to_string(),
                "analysis_fingerprint_fnv1a64": "analysis-hash",
                "data_fingerprint_fnv1a64": "data-hash",
            }))
            .expect("serialize run"),
        )
        .expect("write run");
        fs::write(
            audit_dir.join("evidence-index.json"),
            serde_json::to_string_pretty(&json!({
                "artifacts_dir": out_dir.display().to_string(),
                "query": {
                    "analysis_path": analysis_path.display().to_string(),
                    "data_path": data_path.display().to_string(),
                    "data_fingerprint_fnv1a64": "data-hash",
                    "include_exploratory": false,
                },
                "discovered_runs": [],
                "accepted_artifacts": [
                    {
                        "command": "inspect",
                        "run_dir": run_dir.display().to_string(),
                        "result_path": result_path.display().to_string(),
                        "context_path": context_path.display().to_string(),
                        "status": "accepted",
                        "reason": "matched declared analysis step",
                        "matched_by": "analysis_step",
                        "matched_analysis_step_index": 0,
                        "artifact": {
                            "role": "declared",
                            "status": "produced",
                            "formal_run_id": "run-1",
                            "analysis_step_index": 0,
                        },
                    }
                ],
                "rejected_artifacts": [],
                "notes": [],
            }))
            .expect("serialize evidence"),
        )
        .expect("write evidence");

        out_dir
    }

    fn unstable_logistic_result() -> LogisticResult {
        LogisticResult {
            status: "ok".to_string(),
            validity_status: "unstable".to_string(),
            data_path: "demo.csv".to_string(),
            analysis_path: Some("analysis.yaml".to_string()),
            formula: "logit(disease ~ age)".to_string(),
            outcome: "disease".to_string(),
            predictors: vec!["age".to_string()],
            survey_weight: None,
            n_total: 36,
            n_used: 36,
            n_excluded_missing: 0,
            n_excluded_invalid: 0,
            n_events: 14,
            n_nonevents: 22,
            iterations: 50,
            converged: false,
            log_likelihood: -0.0001,
            null_log_likelihood: None,
            pseudo_r2_nagelkerke: None,
            aic: None,
            bic: None,
            c_statistic: None,
            coefficients: vec![LogisticCoefficient {
                term: "age".to_string(),
                variable: "age".to_string(),
                level: None,
                reference: None,
                beta: 29.2379,
                standard_error: 8064.97,
                odds_ratio: 4_987_598_079_561.157,
                ci_lower: 0.0,
                ci_upper: f64::MAX,
                p_value: 0.9971,
            }],
            notes: vec![],
            diagnostics: vec![Diagnostic::blocking(
                "unstable_confidence_interval",
                "Confidence interval is unstable.",
                None,
            )],
            warnings: vec![
                "model_did_not_converge_within_max_iterations".to_string(),
                "possible_separation_or_extreme_fitted_probabilities".to_string(),
            ],
        }
    }

    #[test]
    fn model_markdown_marks_unstable_intervals_and_warnings() {
        let markdown = super::build_logistic_markdown(&unstable_logistic_result());

        assert!(markdown.contains(
            "- Warnings: model_did_not_converge_within_max_iterations, possible_separation_or_extreme_fitted_probabilities."
        ));
        assert!(markdown.contains("| age | 4.9876e12 | unstable | 0.9971 |"));
        assert!(!markdown.contains("17976931348623157"));
    }

    #[test]
    fn report_markdown_marks_unstable_model_summaries_and_warnings() {
        let spec: AnalysisSpec = serde_yaml::from_str(
            r"
study:
  title: Demo cohort
  design: cohort
data:
  path: demo.csv
  format: csv
analyses:
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
",
        )
        .expect("parse analysis spec");
        let evidence = super::ReportEvidence {
            source_dir: PathBuf::from("runs"),
            logistic: Some(unstable_logistic_result()),
            ..super::ReportEvidence::default()
        };

        let report = super::build_report_markdown_from_evidence(&spec, &evidence);

        assert!(report.contains("age OR 4.99e12 (CI unstable)"));
        assert!(report.contains(
            "Logistic model warnings: model_did_not_converge_within_max_iterations, possible_separation_or_extreme_fitted_probabilities."
        ));
        assert!(!report.contains("17976931348623157"));
    }

    #[test]
    fn report_build_writes_expected_scaffold_files() {
        let root = temp_dir("report");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
study:
  title: Demo cohort
  design: cohort
  population: Adults under surveillance
study_context:
  estimand: 1-year risk ratio
  exposure: Smoking
  comparator: Never smoking
  outcome: Incident disease
  time_zero: Baseline exam date
  follow_up: 12 months
  censoring: Death or loss to follow-up
  missing_data_strategy: Multiple imputation
  clustering: site
  sensitivity_analyses: Alternate exposure coding
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: inspect
  - kind: table_one
    by: disease
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
audit:
  log_dir: epistat-artifacts/audit
  save_commands: true
  save_inputs: true
  save_outputs: true
  save_environment: true
  save_decisions: true
",
        )
        .expect("write analysis yaml");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path.clone(),
                out: Some(out_dir.clone()),
                artifacts: None,
                include_exploratory: false,
            }),
        });

        let rendered = crate::handlers::dispatch(&cli).expect("report build should succeed");
        assert!(rendered.contains("Report Build"));
        assert!(out_dir.join("report").join("methods.md").is_file());
        assert!(out_dir.join("report").join("study-context.md").is_file());
        assert!(out_dir
            .join("report")
            .join("reporting-checklist.md")
            .is_file());
        assert!(out_dir
            .join("audit")
            .join("analysis.normalized.json")
            .is_file());
        assert!(out_dir
            .join("audit")
            .join("analysis_manifest.json")
            .is_file());
        assert!(out_dir.join("audit").join("run.json").is_file());
        assert!(out_dir.join("audit").join("audit-trail.md").is_file());
        assert!(out_dir.join("audit").join("evidence-index.json").is_file());
        let checklist = fs::read_to_string(out_dir.join("report").join("reporting-checklist.md"))
            .expect("read checklist");
        assert!(checklist.contains("STROBE"));
        assert!(checklist.contains("estimand"));
        let manifest = fs::read_to_string(out_dir.join("audit").join("analysis_manifest.json"))
            .expect("read manifest");
        assert!(manifest.contains("\"analysis_fingerprint_fnv1a64\""));
        assert!(manifest.contains("\"reporting\""));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_verify_accepts_evidence_index_with_existing_artifacts() {
        let root = temp_dir("report-verify-ok");
        fs::create_dir_all(&root).expect("create root");
        let out_dir = write_minimal_verified_report_artifacts(&root);

        let result = super::handle_report_verify(&ReportVerifyArgs {
            artifacts: out_dir.clone(),
            fail_on_warning: false,
        });
        assert_eq!(result.status, "ok");
        assert_eq!(result.accepted_count, 1);
        assert_eq!(result.rejected_count, 0);
        assert_eq!(result.error_count, 0);
        assert!(result
            .items
            .iter()
            .any(|item| item.code == "data_fingerprint_matches"));

        let rendered = crate::handlers::dispatch(&test_cli(Command::Report {
            command: ReportCommand::Verify(ReportVerifyArgs {
                artifacts: out_dir,
                fail_on_warning: false,
            }),
        }))
        .expect("report verify should render");
        assert!(rendered.contains("Report Verify"));
        assert!(rendered.contains("Status           ok"));
        assert!(rendered.contains("accepted=1 rejected=0 errors=0"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_verify_reports_missing_accepted_result_path() {
        let root = temp_dir("report-verify-missing-result");
        fs::create_dir_all(&root).expect("create root");
        let out_dir = write_minimal_verified_report_artifacts(&root);
        fs::remove_file(out_dir.join("inspect-main").join("result.json")).expect("remove result");

        let result = super::handle_report_verify(&ReportVerifyArgs {
            artifacts: out_dir,
            fail_on_warning: false,
        });
        assert_eq!(result.status, "error");
        assert!(result.error_count > 0);
        assert!(result
            .items
            .iter()
            .any(|item| item.code == "artifact_result_missing"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_verify_reports_accepted_artifact_with_blocking_diagnostics() {
        let root = temp_dir("report-verify-blocking-diagnostics");
        fs::create_dir_all(&root).expect("create root");
        let out_dir = write_minimal_verified_report_artifacts(&root);
        fs::write(
            out_dir.join("inspect-main").join("result.json"),
            serde_json::to_string_pretty(&json!({
                "status": "ok",
                "diagnostics": [
                    {
                        "code": "unstable_confidence_interval",
                        "severity": "blocking",
                        "message": "Confidence interval is unstable."
                    }
                ]
            }))
            .expect("serialize result"),
        )
        .expect("write result");

        let result = super::handle_report_verify(&ReportVerifyArgs {
            artifacts: out_dir,
            fail_on_warning: false,
        });
        assert_eq!(result.status, "error");
        assert!(result
            .items
            .iter()
            .any(|item| item.code == "accepted_artifact_blocking_diagnostics"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn workflow_report_rejects_logistic_with_blocking_diagnostics() {
        let root = temp_dir("workflow-logistic-blocking-diagnostics");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
schema_version: stats-code.v0
study:
  title: Separation demo
  design: cohort
study_context:
  estimand: Odds ratio
  exposure: Treatment
  comparator: Control
  outcome: Outcome
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: outcome
    kind: binary
    roles: [outcome]
  - name: treatment
    kind: binary
    roles: [exposure]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - id: logistic_sep
    kind: model
    model: logistic
    outcome: outcome
    predictors: [treatment, age]
report:
  out_dir: artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis");
        fs::write(
            root.join("demo.csv"),
            "outcome,treatment,age\n0,0,40\n0,0,42\n0,0,44\n0,0,46\n1,1,50\n1,1,52\n1,1,54\n1,1,56\n",
        )
        .expect("write csv");
        let out_dir = root.join("artifacts");
        crate::handlers::dispatch(&test_cli(Command::Workflow {
            command: WorkflowCommand::Run(WorkflowRunArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                explore_out: None,
                include_exploratory: false,
                strict: false,
                allow_warnings: false,
                allow_unenforced_survey: false,
                allow_unenforced_privacy: false,
                no_chat: true,
            }),
        }))
        .expect("workflow should execute and reject bad formal evidence");

        let step_dir = fs::read_dir(&out_dir)
            .expect("read artifacts")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.join("context.json").is_file())
            .expect("step artifact with context");
        let command_json =
            fs::read_to_string(step_dir.join("command.json")).expect("read command json");
        let context_json =
            fs::read_to_string(step_dir.join("context.json")).expect("read context json");
        assert!(command_json.contains("\"artifact_schema_version\": \"1.0\""));
        assert!(context_json.contains("\"artifact_schema_version\": \"1.0\""));
        assert!(context_json.contains("\"stats_code_version\""));

        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(report_md.contains("Regression models: adjusted effect estimates."));
        assert!(!report_md.contains("2.9804e44"));
        assert!(evidence_index.contains("\"rejected_artifacts\""));
        assert!(evidence_index.contains("artifact has blocking diagnostics"));
        assert!(evidence_index.contains("possible_complete_separation"));
        assert!(evidence_index.contains("\"report_decision\": \"rejected\""));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_only_reports_missing_declared_analyses() {
        let root = temp_dir("report-declared-missing-only");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
study:
  title: Descriptive-only cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: inspect
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        let data_path = root.join("demo.csv");
        fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");
        let artifacts_dir = root.join("runs");
        let tableone_dir = artifacts_dir.join("tableone-1");
        fs::create_dir_all(&tableone_dir).expect("create tableone dir");
        fs::write(
            tableone_dir.join("command.json"),
            r#"{"command":"tableone","request":{}}"#,
        )
        .expect("write tableone command");
        fs::write(
            tableone_dir.join("context.json"),
            serde_json::to_string_pretty(&json!({
                "command": "tableone",
                "analysis_path": analysis_path.display().to_string(),
                "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                "data_path": data_path.display().to_string(),
                "data_path_resolved": resolve_path_for_match(&data_path),
                "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
                "cwd": root.display().to_string(),
            }))
            .expect("serialize tableone context"),
        )
        .expect("write tableone context");
        fs::write(
            tableone_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71); median 1.50 [1.00, 2.00]","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00 (NA); median 1.00 [1.00, 1.00]","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00 (NA); median 2.00 [2.00, 2.00]","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "test_name":"Welch_t_test",
      "p_value":0.0,
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
        )
        .expect("write tableone result");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                artifacts: Some(artifacts_dir),
                include_exploratory: false,
            }),
        });

        crate::handlers::dispatch(&cli).expect("report build should succeed");
        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        assert!(report_md.contains("Table 1 available for `category`"));
        assert!(!report_md.contains("Rate analysis: no observed result found."));
        assert!(!report_md.contains("Logistic model: no observed result found."));
        assert!(!report_md.contains("Cox model: no observed result found."));
        let table_md =
            fs::read_to_string(out_dir.join("tables").join("tableone.md")).expect("read table");
        assert!(table_md.contains("data_value"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_selects_tableone_matching_declared_by() {
        let root = temp_dir("report-tableone-declared-by");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
study:
  title: Descriptive-only cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: year
    kind: categorical
    roles: [strata]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: inspect
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        let data_path = root.join("demo.csv");
        fs::write(
            &data_path,
            "category,year,data_value\nA,2022,1.0\nB,2023,2.0\n",
        )
        .expect("write csv");
        let data_fingerprint = fingerprint_file(&data_path).expect("fingerprint");
        let artifacts_dir = root.join("runs");
        let category_dir = artifacts_dir.join("tableone-category");
        let year_dir = artifacts_dir.join("tableone-year");
        fs::create_dir_all(&category_dir).expect("create category dir");
        fs::create_dir_all(&year_dir).expect("create year dir");

        for run_dir in [&category_dir, &year_dir] {
            fs::write(
                run_dir.join("command.json"),
                r#"{"command":"tableone","request":{}}"#,
            )
            .expect("write tableone command");
            fs::write(
                run_dir.join("context.json"),
                serde_json::to_string_pretty(&json!({
                    "command": "tableone",
                    "analysis_path": analysis_path.display().to_string(),
                    "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                    "data_path": data_path.display().to_string(),
                    "data_path_resolved": resolve_path_for_match(&data_path),
                    "data_fingerprint_fnv1a64": data_fingerprint,
                    "cwd": root.display().to_string(),
                }))
                .expect("serialize context"),
            )
            .expect("write context");
        }

        fs::write(
            category_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
        )
        .expect("write category result");
        fs::write(
            year_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"year",
  "group_levels":["2022","2023"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"2022","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"2023","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
        )
        .expect("write year result");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                artifacts: Some(artifacts_dir),
                include_exploratory: false,
            }),
        });

        crate::handlers::dispatch(&cli).expect("report build should succeed");
        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        let table_md =
            fs::read_to_string(out_dir.join("tables").join("tableone.md")).expect("read table");
        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(report_md.contains("Table 1 available for `category`"));
        assert!(!report_md.contains("Table 1 available for `year`"));
        assert!(table_md.contains("| Variable | Overall | A | B |"));
        assert!(!table_md.contains("| Variable | Overall | 2022 | 2023 |"));
        assert!(evidence_index.contains("\"accepted_artifacts\""));
        assert!(evidence_index.contains("\"rejected_artifacts\""));
        assert!(evidence_index.contains("artifact does not match a declared analysis step"));
        assert!(evidence_index.contains("\"matched_analysis_step_index\": 1"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_rejects_exploratory_artifacts_by_default() {
        let root = temp_dir("report-exploratory-filter");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
study:
  title: Exploratory filter cohort
  design: cross-sectional
study_context:
  estimand: Descriptive prevalence summaries
  exposure: Category
  comparator: Other categories
  outcome: Prevalence
  missing_data_strategy: Report missing values
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: category
    kind: categorical
    roles: [exposure]
  - name: data_value
    kind: continuous
    roles: [outcome]
analyses:
  - kind: table_one
    by: category
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        let data_path = root.join("demo.csv");
        fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");

        let artifacts_dir = root.join("runs");
        let tableone_dir = artifacts_dir.join("tableone-explore");
        fs::create_dir_all(&tableone_dir).expect("create tableone dir");
        fs::write(
            tableone_dir.join("command.json"),
            r#"{"command":"tableone","request":{}}"#,
        )
        .expect("write tableone command");
        fs::write(
            tableone_dir.join("context.json"),
            serde_json::to_string_pretty(&json!({
                "command": "tableone",
                "analysis_path": analysis_path.display().to_string(),
                "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                "data_path": data_path.display().to_string(),
                "data_path_resolved": resolve_path_for_match(&data_path),
                "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
                "cwd": root.display().to_string(),
                "artifact": {
                    "role": "exploratory",
                    "status": "produced"
                }
            }))
            .expect("serialize context"),
        )
        .expect("write context");
        fs::write(
            tableone_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "by":"category",
  "group_levels":["A","B"],
  "rows":[
    {
      "variable":"data_value",
      "kind":"continuous",
      "overall":{"display":"1.50 (0.71)","n_total":2,"n_non_missing":2,"missing_count":0},
      "groups":[
        {"group":"A","cell":{"display":"1.00","n_total":1,"n_non_missing":1,"missing_count":0}},
        {"group":"B","cell":{"display":"2.00","n_total":1,"n_non_missing":1,"missing_count":0}}
      ],
      "warnings":[]
    }
  ],
  "notes":[]
}"#,
        )
        .expect("write result");

        let out_dir = root.join("formal-report");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path,
                out: Some(out_dir.clone()),
                artifacts: Some(artifacts_dir),
                include_exploratory: false,
            }),
        });

        crate::handlers::dispatch(&cli).expect("report build should reject exploratory evidence");
        assert!(!out_dir.join("tables").join("tableone.md").exists());
        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(evidence_index.contains("\"rejected_artifacts\""));
        assert!(evidence_index.contains("exploratory artifact was not requested"));
        assert!(evidence_index.contains("\"role\": \"exploratory\""));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_consumes_observed_result_artifacts() {
        let root = temp_dir("report-evidence");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        let data_path = root.join("demo.csv");
        fs::write(
            &analysis_path,
            r"
study:
  title: Demo cohort
  design: cohort
study_context:
  estimand: 1-year rate ratio and odds ratio
  outcome: Incident disease
  time_zero: Baseline visit
  follow_up: 12 months
  censoring: End of follow-up
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: rate
    event: disease
    person_time: fu_pt
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        fs::write(&data_path, "disease,fu_pt,age,sex\n1,1.0,50,female\n").expect("write csv");
        let data_fingerprint = fingerprint_file(&data_path).expect("fingerprint");

        let artifacts_dir = root.join("runs");
        let logistic_dir = artifacts_dir.join("model_logistic-1");
        let rate_dir = artifacts_dir.join("rate-1");
        fs::create_dir_all(&logistic_dir).expect("create logistic dir");
        fs::create_dir_all(&rate_dir).expect("create rate dir");
        fs::write(
            logistic_dir.join("command.json"),
            r#"{"command":"model_logistic","request":{}}"#,
        )
        .expect("write logistic command");
        fs::write(
            logistic_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":100,
  "n_used":96,
  "n_excluded_missing":4,
  "n_excluded_invalid":0,
  "n_events":24,
  "n_nonevents":72,
  "iterations":5,
  "converged":true,
  "log_likelihood":-48.12,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-2.1,
      "standard_error":0.8,
      "odds_ratio":0.1225,
      "ci_lower":0.025,
      "ci_upper":0.600,
      "p_value":0.01
    },
    {
      "term":"age",
      "variable":"age",
      "beta":0.08,
      "standard_error":0.03,
      "odds_ratio":1.0833,
      "ci_lower":1.02,
      "ci_upper":1.15,
      "p_value":0.008
    }
  ],
  "notes":["demo logistic"],
  "warnings":[]
}"#,
        )
        .expect("write logistic result");
        fs::write(
            logistic_dir.join("context.json"),
            serde_json::to_string_pretty(&json!({
                "command": "model_logistic",
                "analysis_path": analysis_path.display().to_string(),
                "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                "data_path": data_path.display().to_string(),
                "data_path_resolved": resolve_path_for_match(&data_path),
                "data_fingerprint_fnv1a64": data_fingerprint,
                "cwd": root.display().to_string(),
            }))
            .expect("serialize logistic context"),
        )
        .expect("write logistic context");
        fs::write(
            rate_dir.join("command.json"),
            r#"{"command":"rate","request":{}}"#,
        )
        .expect("write rate command");
        fs::write(
            rate_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "event":"disease",
  "person_time":"fu_pt",
  "strata":["sex"],
  "rows":[
    {
      "stratum":"sex=female",
      "total_records":50,
      "included_records":50,
      "events":10.0,
      "person_time":120.0,
      "rate":0.083333,
      "rate_per_1000":83.333,
      "lower_ci_per_1000":40.000,
      "upper_ci_per_1000":150.000
    }
  ],
  "notes":["demo rate"]
}"#,
        )
        .expect("write rate result");
        fs::write(
            rate_dir.join("context.json"),
            serde_json::to_string_pretty(&json!({
                "command": "rate",
                "analysis_path": analysis_path.display().to_string(),
                "analysis_path_resolved": resolve_path_for_match(&analysis_path),
                "data_path": data_path.display().to_string(),
                "data_path_resolved": resolve_path_for_match(&data_path),
                "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("fingerprint"),
                "cwd": root.display().to_string(),
            }))
            .expect("serialize rate context"),
        )
        .expect("write rate context");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path.clone(),
                out: Some(out_dir.clone()),
                artifacts: Some(artifacts_dir.clone()),
                include_exploratory: false,
            }),
        });

        let rendered =
            crate::handlers::dispatch(&cli).expect("report build should consume evidence");
        assert!(rendered.contains("Report Build"));
        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        assert!(report_md.contains("age OR 1.08"));
        assert!(report_md.contains("sex=female = 83.33/1000"));
        assert!(out_dir
            .join("tables")
            .join("model-logistic-summary.md")
            .is_file());
        assert!(out_dir.join("tables").join("rate-summary.md").is_file());
        assert!(out_dir.join("audit").join("evidence-index.json").is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_ignores_mismatched_result_artifacts() {
        let root = temp_dir("report-evidence-mismatch");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        let data_path = root.join("demo.csv");
        let other_data_path = root.join("other.csv");
        fs::write(
            &analysis_path,
            r"
study:
  title: Demo cohort
  design: cohort
study_context:
  estimand: Adjusted odds ratio
  outcome: Incident disease
  missing_data_strategy: Complete-case analysis
  reporting_guideline: STROBE
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
  - name: age
    kind: continuous
    roles: [covariate]
analyses:
  - kind: model
    model: logistic
    outcome: disease
    predictors: [age]
report:
  out_dir: epistat-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        fs::write(&data_path, "disease,age\n1,50\n0,40\n").expect("write primary csv");
        fs::write(&other_data_path, "disease,age\n1,80\n1,78\n").expect("write other csv");

        let artifacts_dir = root.join("runs");
        let matching_dir = artifacts_dir.join("model_logistic-match");
        let mismatched_dir = artifacts_dir.join("model_logistic-mismatch");
        fs::create_dir_all(&matching_dir).expect("create match dir");
        fs::create_dir_all(&mismatched_dir).expect("create mismatch dir");

        let matching_context = json!({
            "command": "model_logistic",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&data_path),
            "data_fingerprint_fnv1a64": fingerprint_file(&data_path).expect("primary fingerprint"),
            "cwd": root.display().to_string(),
        });
        let mismatched_context = json!({
            "command": "model_logistic",
            "analysis_path": analysis_path.display().to_string(),
            "analysis_path_resolved": resolve_path_for_match(&analysis_path),
            "data_path": other_data_path.display().to_string(),
            "data_path_resolved": resolve_path_for_match(&other_data_path),
            "data_fingerprint_fnv1a64": fingerprint_file(&other_data_path).expect("other fingerprint"),
            "cwd": root.display().to_string(),
        });

        fs::write(
            matching_dir.join("command.json"),
            r#"{"command":"model_logistic","request":{}}"#,
        )
        .expect("write matching command");
        fs::write(
            matching_dir.join("context.json"),
            serde_json::to_string_pretty(&matching_context).expect("serialize match context"),
        )
        .expect("write matching context");
        fs::write(
            matching_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"demo.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":100,
  "n_used":96,
  "n_excluded_missing":4,
  "n_excluded_invalid":0,
  "n_events":24,
  "n_nonevents":72,
  "iterations":5,
  "converged":true,
  "log_likelihood":-48.12,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-2.1,
      "standard_error":0.8,
      "odds_ratio":0.1225,
      "ci_lower":0.025,
      "ci_upper":0.600,
      "p_value":0.01
    },
    {
      "term":"age",
      "variable":"age",
      "beta":0.08,
      "standard_error":0.03,
      "odds_ratio":1.0833,
      "ci_lower":1.02,
      "ci_upper":1.15,
      "p_value":0.008
    }
  ],
  "notes":["matching logistic"],
  "warnings":[]
}"#,
        )
        .expect("write matching result");

        fs::write(
            mismatched_dir.join("command.json"),
            r#"{"command":"model_logistic","request":{}}"#,
        )
        .expect("write mismatched command");
        fs::write(
            mismatched_dir.join("context.json"),
            serde_json::to_string_pretty(&mismatched_context).expect("serialize mismatch context"),
        )
        .expect("write mismatched context");
        fs::write(
            mismatched_dir.join("result.json"),
            r#"{
  "status":"ok",
  "data_path":"other.csv",
  "analysis_path":"analysis.yaml",
  "formula":"logit(disease ~ age)",
  "outcome":"disease",
  "predictors":["age"],
  "n_total":40,
  "n_used":40,
  "n_excluded_missing":0,
  "n_excluded_invalid":0,
  "n_events":30,
  "n_nonevents":10,
  "iterations":8,
  "converged":true,
  "log_likelihood":-10.00,
  "coefficients":[
    {
      "term":"Intercept",
      "variable":"Intercept",
      "beta":-0.5,
      "standard_error":0.5,
      "odds_ratio":0.6065,
      "ci_lower":0.22,
      "ci_upper":1.66,
      "p_value":0.32
    },
    {
      "term":"age",
      "variable":"age",
      "beta":1.5041,
      "standard_error":0.4,
      "odds_ratio":4.5000,
      "ci_lower":2.00,
      "ci_upper":9.00,
      "p_value":0.0001
    }
  ],
  "notes":["mismatched logistic"],
  "warnings":[]
}"#,
        )
        .expect("write mismatched result");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path.clone(),
                out: Some(out_dir.clone()),
                artifacts: Some(artifacts_dir.clone()),
                include_exploratory: false,
            }),
        });

        crate::handlers::dispatch(&cli).expect("report build should filter mismatched evidence");
        let report_md =
            fs::read_to_string(out_dir.join("report").join("report.md")).expect("read report");
        let evidence_index = fs::read_to_string(out_dir.join("audit").join("evidence-index.json"))
            .expect("read evidence index");
        assert!(report_md.contains("age OR 1.08"));
        assert!(!report_md.contains("age OR 4.50"));
        assert!(evidence_index.contains("data_fingerprint"));
        assert!(evidence_index.contains("did not match the current analysis/data identity"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_build_rejects_missing_required_study_context() {
        let root = temp_dir("report-missing-study-context");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
study:
  title: Demo cohort
  design: cohort
data:
  path: demo.csv
  format: csv
variables:
  - name: disease
    kind: binary
    roles: [outcome]
analyses:
  - kind: table_one
    by: disease
",
        )
        .expect("write analysis yaml");
        fs::write(root.join("demo.csv"), "disease\n1\n0\n").expect("write csv");

        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path,
                out: Some(root.join("artifacts")),
                artifacts: None,
                include_exploratory: false,
            }),
        });

        let error = crate::handlers::dispatch(&cli).expect_err("report build should fail");
        assert!(error.contains("study_context"));
        assert!(error.contains("estimand"));
        assert!(error.contains("reporting_guideline"));
        assert!(error.contains("Suggested template"));
        assert!(error.contains("study_context:"));
        assert!(error.contains("outcome: \"disease\""));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
