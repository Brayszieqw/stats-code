use std::collections::BTreeSet;
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
    build_reporting_checklist_markdown, build_study_context_markdown, build_variables_markdown,
};
use crate::schema::{
    load_analysis_spec, validate_study_context, AnalysisCheckItem, AnalysisCheckLevel,
    AnalysisSpec, ArtifactMetadata, ArtifactRole, ArtifactStatus, CoxResult, InspectResult,
    LinearResult, LogisticResult, ModelKind, RateResult, ReportBuildResult, ReportVerifyResult,
    TableOneResult, VariableRole,
};
mod artifacts;
mod evidence;
mod markdown;
mod study;
mod verify;

#[cfg(test)]
mod tests;

pub(crate) use artifacts::persist_run_artifacts_with_metadata;
pub(crate) use study::ensure_study_context_ready;
pub(crate) use verify::handle_report_verify;

use evidence::{discover_report_evidence, report_artifact_decision_json};
use markdown::{
    build_cox_markdown, build_linear_markdown, build_logistic_markdown, build_rate_markdown,
    build_report_markdown_from_evidence, build_tableone_markdown,
    build_tables_readme_from_evidence, small_cell_threshold, write_report_file,
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
