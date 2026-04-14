use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::helpers::{
    extract_string_field, fingerprint_file, path_matches, resolve_path_for_match,
    resolve_path_str_for_match, stringify_error, unix_timestamp_nanos,
};
use crate::render::{
    build_analysis_manifest, build_assumptions_markdown, build_audit_trail_markdown,
    build_command_log, build_methods_markdown, build_report_markdown,
    build_reporting_checklist_markdown, build_study_context_markdown, build_tables_readme,
    build_variables_markdown,
};
use crate::schema::{
    load_analysis_spec, validate_study_context, AnalysisSpec, CoxResult, InspectResult,
    LinearResult, LogisticResult, RateResult, ReportBuildResult, TableOneResult, VariableRole,
};
use crate::cli::ReportBuildArgs;

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
}

impl ReportEvidenceQuery {
    pub fn new(analysis_path: &Path, data_path: &Path) -> Self {
        Self {
            analysis_path_resolved: resolve_path_for_match(analysis_path),
            data_path_resolved: resolve_path_for_match(data_path),
            data_fingerprint_fnv1a64: fingerprint_file(data_path),
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunArtifactContext {
    pub command: String,
    #[serde(default)]
    pub analysis_path: Option<String>,
    #[serde(default)]
    pub analysis_path_resolved: Option<String>,
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
    let artifacts_dir = args
        .artifacts
        .as_ref().map_or_else(|| out_dir.clone(), |path| resolve_relative_to_analysis(&analysis_path, path));
    let report_dir = out_dir.join("report");
    let tables_dir = out_dir.join("tables");
    let audit_dir = out_dir.join("audit");
    let data_path = resolve_relative_to_analysis(&analysis_path, &spec.data.path);
    let evidence_query = ReportEvidenceQuery::new(&analysis_path, &data_path);
    fs::create_dir_all(&report_dir).map_err(stringify_error)?;
    fs::create_dir_all(&tables_dir).map_err(stringify_error)?;
    fs::create_dir_all(&audit_dir).map_err(stringify_error)?;

    let evidence = discover_report_evidence(&artifacts_dir, &evidence_query)?;

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
        },
        "discovered_runs": evidence.discovered_runs.iter().map(|run| json!({
            "command": &run.command,
            "run_dir": &run.run_dir,
            "result_path": &run.result_path,
            "context_path": &run.context_path,
            "matched_by": &run.matched_by,
        })).collect::<Vec<_>>(),
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
            &build_tableone_markdown(tableone),
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

// ---------------------------------------------------------------------------
// Study context validation & template
// ---------------------------------------------------------------------------

pub(crate) fn ensure_study_context_ready(analysis_path: &Path, spec: &AnalysisSpec) -> Result<(), String> {
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

    let analysis_path = if let Some(path) = explicit_analysis.cloned() { path } else {
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
            .parent().map_or_else(|| path.to_path_buf(), |parent| parent.join(path))
    }
}

// ---------------------------------------------------------------------------
// Evidence discovery
// ---------------------------------------------------------------------------

fn discover_report_evidence(
    source_dir: &Path,
    query: &ReportEvidenceQuery,
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
        let Some(matched_by) = match_report_artifact(
            query,
            &command_name,
            command_value.get("request"),
            &result_value,
            context.as_ref(),
        ) else {
            evidence.notes.push(format!(
                "Skipped `{}` because it did not match the current analysis/data identity.",
                path.display()
            ));
            continue;
        };
        let discovered = DiscoveredRunArtifact {
            command: command_name.clone(),
            run_dir: path.display().to_string(),
            result_path: result_path.display().to_string(),
            context_path: context_path
                .is_file()
                .then(|| context_path.display().to_string()),
            matched_by,
        };
        match command_name.as_str() {
            "inspect" => {
                if let Ok(value) = serde_json::from_value::<InspectResult>(result_value.clone()) {
                    evidence.inspect = Some(value);
                    evidence.discovered_runs.push(discovered);
                }
            }
            "tableone" => {
                if let Ok(value) = serde_json::from_value::<TableOneResult>(result_value.clone()) {
                    evidence.tableone = Some(value);
                    evidence.discovered_runs.push(discovered);
                }
            }
            "rate" => {
                if let Ok(value) = serde_json::from_value::<RateResult>(result_value.clone()) {
                    evidence.rate = Some(value);
                    evidence.discovered_runs.push(discovered);
                }
            }
            "model_logistic" => {
                if let Ok(value) = serde_json::from_value::<LogisticResult>(result_value.clone()) {
                    evidence.logistic = Some(value);
                    evidence.discovered_runs.push(discovered);
                }
            }
            "model_cox" => {
                if let Ok(value) = serde_json::from_value::<CoxResult>(result_value) {
                    evidence.cox = Some(value);
                    evidence.discovered_runs.push(discovered);
                }
            }
            "model_linear" => {
                if let Ok(value) = serde_json::from_value::<LinearResult>(result_value) {
                    evidence.linear = Some(value);
                    evidence.discovered_runs.push(discovered);
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
    } else {
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
    } else {
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
                    "{} OR {:.2} [{:.2}, {:.2}]",
                    coefficient.term,
                    coefficient.odds_ratio,
                    coefficient.ci_lower,
                    coefficient.ci_upper
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
    } else {
        let _ = writeln!(out, "- Logistic model: no observed result found.");
    }
    if let Some(cox) = &evidence.cox {
        let top_terms = cox
            .coefficients
            .iter()
            .take(3)
            .map(|coefficient| {
                format!(
                    "{} HR {:.2} [{:.2}, {:.2}]",
                    coefficient.term,
                    coefficient.hazard_ratio,
                    coefficient.ci_lower,
                    coefficient.ci_upper
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
    } else {
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
                    coefficient.term,
                    coefficient.beta,
                    coefficient.ci_lower,
                    coefficient.ci_upper
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

fn build_tableone_markdown(result: &TableOneResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Table 1");
    let _ = writeln!(out);
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
            .as_ref().map_or_else(|| label.to_string(), |level| format!("{label} = {level}"));
        let group_cells = result
            .group_levels
            .iter()
            .map(|group| {
                row.groups
                    .iter()
                    .find(|cell| &cell.group == group).map_or_else(|| "NA".to_string(), |cell| cell.cell.display.clone())
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(
            out,
            "| {name} | {} | {} |",
            row.overall.display, group_cells
        );
    }
    out
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
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | OR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let _ = writeln!(
            out,
            "| {} | {:.4} | [{:.4}, {:.4}] | {:.4} |",
            coefficient.term,
            coefficient.odds_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            coefficient.p_value
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
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | HR | 95% CI | p-value |");
    let _ = writeln!(out, "| --- | ---: | --- | ---: |");
    for coefficient in &result.coefficients {
        let _ = writeln!(
            out,
            "| {} | {:.4} | [{:.4}, {:.4}] | {:.4} |",
            coefficient.term,
            coefficient.hazard_ratio,
            coefficient.ci_lower,
            coefficient.ci_upper,
            coefficient.p_value
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
    let _ = writeln!(out, "- R²: {:.4}, Adjusted R²: {:.4}", result.r_squared, result.adjusted_r_squared);
    if let Some(f) = result.f_statistic {
        let p_text = result.f_p_value.map(|p| format!(", p={p:.4}")).unwrap_or_default();
        let _ = writeln!(out, "- F-statistic: {f:.4}{p_text}");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "| Term | β | SE | t | p-value | 95% CI |");
    let _ = writeln!(out, "| --- | ---: | ---: | ---: | ---: | --- |");
    for coefficient in &result.coefficients {
        let _ = writeln!(
            out,
            "| {} | {:.4} | {:.4} | {:.4} | {:.4} | [{:.4}, {:.4}] |",
            coefficient.term,
            coefficient.beta,
            coefficient.standard_error,
            coefficient.t_statistic,
            coefficient.p_value,
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

pub(crate) fn persist_run_artifacts(
    base_dir: &Path,
    command_name: &str,
    request: &Value,
    response: &Value,
) -> Result<(), String> {
    let run_dir = base_dir.join(format!("{}-{}", command_name, unix_timestamp_nanos()));
    fs::create_dir_all(&run_dir).map_err(stringify_error)?;
    fs::write(
        run_dir.join("command.json"),
        serde_json::to_string_pretty(&json!({
            "command": command_name,
            "request": request,
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
        serde_json::to_string_pretty(&build_run_artifact_context(command_name, request, response))
            .map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    Ok(())
}

fn build_run_artifact_context(
    command_name: &str,
    request: &Value,
    response: &Value,
) -> RunArtifactContext {
    let cwd = std::env::current_dir().ok();
    let analysis_path = extract_string_field(response, &["analysis_path"])
        .or_else(|| extract_string_field(request, &["analysis"]));
    let data_path = extract_string_field(response, &["data_path"])
        .or_else(|| extract_string_field(request, &["data", "data_path"]));

    RunArtifactContext {
        command: command_name.to_string(),
        analysis_path: analysis_path.clone(),
        analysis_path_resolved: analysis_path
            .as_deref()
            .map(|path| resolve_path_str_for_match(path, cwd.as_deref())),
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

    use crate::cli::{Command, ReportBuildArgs, ReportCommand};
    use crate::helpers::{fingerprint_file, resolve_path_for_match};

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

    #[test]
    fn report_build_writes_expected_scaffold_files() {
        let root = temp_dir("report");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r#"
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
"#,
        )
        .expect("write analysis yaml");

        let out_dir = root.join("artifacts");
        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path.clone(),
                out: Some(out_dir.clone()),
                artifacts: None,
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
    fn report_build_consumes_observed_result_artifacts() {
        let root = temp_dir("report-evidence");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        let data_path = root.join("demo.csv");
        fs::write(
            &analysis_path,
            r#"
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
"#,
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
            }),
        });

        let rendered = crate::handlers::dispatch(&cli).expect("report build should consume evidence");
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
            r#"
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
"#,
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
            r#"
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
"#,
        )
        .expect("write analysis yaml");
        fs::write(root.join("demo.csv"), "disease\n1\n0\n").expect("write csv");

        let cli = test_cli(Command::Report {
            command: ReportCommand::Build(ReportBuildArgs {
                analysis: analysis_path,
                out: Some(root.join("artifacts")),
                artifacts: None,
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
