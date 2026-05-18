use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::bridge::Engine;
use crate::cli::{
    InspectArgs, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, NaStrategy, RateArgs,
    ReportVerifyArgs, TableOneArgs, WorkflowRunArgs,
};
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::render::render_analysis_check_text;
use crate::report::{
    ensure_study_context_ready, handle_report_build, handle_report_verify,
    persist_run_artifacts_with_metadata, resolve_relative_to_analysis,
};
use crate::schema::{
    load_analysis_spec, AnalysisKind, AnalysisSpec, ArtifactMetadata, ModelKind, WorkflowRunResult,
    WorkflowStepRunResult,
};
use crate::stats::basic::{
    attributable_csv, cochran_armitage_csv, lifetable_csv, lifetable_individual_csv,
    mann_whitney_csv, mcnemar_csv, normality_csv, oneway_anova_csv, or_rr_csv, rbd_anova_csv,
    standardize_csv, variance_homogeneity_csv, wilcoxon_csv,
};
use crate::stats::correlation::correlation_csv;
use crate::stats::ttest::{one_sample_ttest_csv, paired_ttest_csv};

use super::analysis::{
    privacy_requires_policy_exception, survey_requires_policy_exception, validate_analysis_contract,
};
use super::data::{handle_inspect, handle_rate, handle_tableone};
use super::model::{handle_model_cox, handle_model_linear, handle_model_logistic};
pub(crate) fn handle_workflow_run(
    args: &WorkflowRunArgs,
    engine: Engine,
) -> Result<WorkflowRunResult, String> {
    let analysis_path = args.analysis.canonicalize().map_err(stringify_error)?;
    let spec = load_analysis_spec(&analysis_path)?;
    let check = validate_analysis_contract(&analysis_path, &spec);
    if check.has_errors() {
        return Err(render_analysis_check_text(&check));
    }
    ensure_study_context_ready(&analysis_path, &spec)?;
    let policy_exceptions = workflow_policy_exceptions(args, &spec)?;
    let data_path = resolve_relative_to_analysis(&analysis_path, &spec.data.path);
    let out_dir = args
        .out
        .as_ref()
        .map(|path| resolve_relative_to_analysis(&analysis_path, path))
        .or_else(|| {
            spec.report
                .as_ref()
                .map(|report| resolve_relative_to_analysis(&analysis_path, &report.out_dir))
        })
        .unwrap_or_else(|| {
            analysis_path.parent().map_or_else(
                || PathBuf::from("stats-code-artifacts"),
                |parent| parent.join("stats-code-artifacts"),
            )
        });
    fs::create_dir_all(&out_dir).map_err(stringify_error)?;

    let run_id = format!("formal-{}", unix_timestamp_nanos());
    let mut steps = Vec::new();

    for (index, step) in spec.analyses.iter().enumerate() {
        let (command, request, response) =
            execute_workflow_step(index, step, &analysis_path, &data_path, engine)?;
        let artifact = ArtifactMetadata::declared(&run_id, index);
        let artifact_dir = persist_run_artifacts_with_metadata(
            &out_dir,
            &command,
            &request,
            &response,
            Some(&artifact),
        )?;
        steps.push(WorkflowStepRunResult {
            step_index: index,
            command,
            artifact_dir: artifact_dir.display().to_string(),
            status: response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ok")
                .to_string(),
            notes: vec!["Formal declared analysis step executed by workflow run.".to_string()],
        });
    }

    let report = handle_report_build(&crate::cli::ReportBuildArgs {
        analysis: analysis_path.clone(),
        out: Some(out_dir.clone()),
        artifacts: Some(out_dir.clone()),
        include_exploratory: args.include_exploratory,
    })?;
    record_workflow_policy_exceptions(&out_dir, &policy_exceptions)?;
    let strict_policy_notes = enforce_workflow_report_policy(args, &out_dir)?;

    let mut notes = vec![
        "Executed declared analysis.yaml steps in order.".to_string(),
        "Formal artifacts were written with declared workflow metadata.".to_string(),
    ];
    for exception in &policy_exceptions {
        if let Some(message) = exception.get("message").and_then(Value::as_str) {
            notes.push(format!("Policy exception: {message}"));
        }
    }
    notes.extend(strict_policy_notes);
    if let Some(path) = &args.explore_out {
        notes.push(format!(
            "Exploratory artifact directory is separate and was not used for formal evidence: {}.",
            path.display()
        ));
    }
    if args.no_chat {
        notes.push("Chat orchestration was skipped; workflow execution was CLI-only.".to_string());
    }

    Ok(WorkflowRunResult {
        status: "ok".to_string(),
        run_id,
        analysis_path: analysis_path.display().to_string(),
        data_path: data_path.display().to_string(),
        artifacts_dir: out_dir.display().to_string(),
        report_output_dir: report.output_dir.clone(),
        steps,
        report,
        notes,
    })
}

fn workflow_policy_exceptions(
    args: &WorkflowRunArgs,
    spec: &AnalysisSpec,
) -> Result<Vec<Value>, String> {
    let mut exceptions = Vec::new();
    if let Some(survey) = &spec.survey {
        let needs_exception = survey_requires_policy_exception(survey);
        if args.strict && needs_exception && !args.allow_unenforced_survey {
            return Err(
                "ERROR: complex survey variance metadata was declared but strata/cluster/replicate-weight variance is not implemented for this workflow.\nThis workflow cannot produce a strict formal report unless --allow-unenforced-survey is set."
                    .to_string(),
            );
        }
        if args.allow_unenforced_survey && needs_exception {
            exceptions.push(json!({
                "code": "unsupported_complex_survey_variance",
                "allowed_by_user": true,
                "message": "Complex survey variance metadata was declared, but strata/cluster/replicate-weight variance was not implemented in this run; supported engines apply weights to point estimates only."
            }));
        }
    }

    if let Some(privacy) = &spec.privacy {
        let needs_exception = privacy_requires_policy_exception(privacy);
        if args.strict && needs_exception && !args.allow_unenforced_privacy {
            return Err(
                "ERROR: privacy metadata requiring de-identification or identifier handling was declared but is not implemented for this workflow.\nThis workflow cannot produce a strict formal report unless --allow-unenforced-privacy is set."
                    .to_string(),
            );
        }
        if args.allow_unenforced_privacy && needs_exception {
            exceptions.push(json!({
                "code": "unenforced_privacy_policy",
                "allowed_by_user": true,
                "message": "Privacy metadata requiring de-identification or identifier handling was declared, but those controls were not enforced in this run."
            }));
        }
    }

    Ok(exceptions)
}

fn record_workflow_policy_exceptions(out_dir: &Path, exceptions: &[Value]) -> Result<(), String> {
    if exceptions.is_empty() {
        return Ok(());
    }

    let evidence_path = out_dir.join("audit").join("evidence-index.json");
    let evidence_text = fs::read_to_string(&evidence_path).map_err(stringify_error)?;
    let mut evidence: Value = serde_json::from_str(&evidence_text).map_err(stringify_error)?;
    evidence["policy_exceptions"] = Value::Array(exceptions.to_vec());
    if let Some(notes) = evidence.get_mut("notes").and_then(Value::as_array_mut) {
        for exception in exceptions {
            if let Some(message) = exception.get("message").and_then(Value::as_str) {
                notes.push(Value::String(format!("Policy exception: {message}")));
            }
        }
    }
    fs::write(
        &evidence_path,
        serde_json::to_string_pretty(&evidence).map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;

    let report_path = out_dir.join("report").join("report.md");
    if report_path.is_file() {
        let mut report = fs::read_to_string(&report_path).map_err(stringify_error)?;
        report.push_str("\n## Policy Exceptions\n\n");
        for exception in exceptions {
            if let Some(message) = exception.get("message").and_then(Value::as_str) {
                let _ = writeln!(report, "- {message}");
            }
        }
        fs::write(&report_path, report).map_err(stringify_error)?;
    }

    Ok(())
}

fn enforce_workflow_report_policy(
    args: &WorkflowRunArgs,
    out_dir: &Path,
) -> Result<Vec<String>, String> {
    if !args.strict {
        return Ok(Vec::new());
    }

    let verify = handle_report_verify(&ReportVerifyArgs {
        artifacts: out_dir.to_path_buf(),
        fail_on_warning: false,
    });
    if verify.has_errors() {
        return Err(format!(
            "Strict workflow policy failed: report verify found {} error(s) in `{}`.",
            verify.error_count,
            out_dir.display()
        ));
    }
    if verify.warning_count > 0 && !args.allow_warnings {
        return Err(format!(
            "Strict workflow policy failed: report verify found {} warning(s) in `{}`.\nUse --allow-warnings only for internal exploratory or explicitly reviewed runs.",
            verify.warning_count,
            out_dir.display()
        ));
    }

    let mut notes = vec!["Strict workflow policy was evaluated after report build.".to_string()];
    if verify.warning_count > 0 {
        notes.push(format!(
            "--allow-warnings allowed {} report verification warning(s) for this run.",
            verify.warning_count
        ));
    }
    Ok(notes)
}

fn execute_workflow_step(
    index: usize,
    step: &crate::schema::AnalysisStepSpec,
    analysis_path: &Path,
    data_path: &Path,
    engine: Engine,
) -> Result<(String, serde_json::Value, serde_json::Value), String> {
    match step.kind {
        AnalysisKind::Inspect => {
            let args = InspectArgs {
                data_path: data_path.to_path_buf(),
            };
            let result = handle_inspect(&args)?;
            Ok((
                "inspect".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::TableOne => {
            let by = required_step_field(index, "by", step.by.as_deref())?;
            let args = TableOneArgs {
                data: None,
                analysis: Some(analysis_path.to_path_buf()),
                by,
                vars: Vec::new(),
            };
            let result = handle_tableone(&args)?;
            Ok((
                "tableone".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::Rate => {
            let event = required_step_field(index, "event", step.event.as_deref())?;
            let person_time =
                required_step_field(index, "person_time", step.person_time.as_deref())?;
            let args = RateArgs {
                data: None,
                analysis: Some(analysis_path.to_path_buf()),
                event,
                person_time,
                strata: step.strata.clone(),
            };
            let result = handle_rate(&args)?;
            Ok((
                "rate".to_string(),
                json!(args),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::TtestPaired => {
            let before = required_step_field(index, "before", step.before.as_deref())?;
            let after = required_step_field(index, "after", step.after.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = paired_ttest_csv(&rows, &headers, &before, &after, 0.05)?;
            Ok((
                "stats.ttest.paired".to_string(),
                json!({
                    "kind": "ttest.paired",
                    "before": before,
                    "after": after,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::TtestOneSample => {
            let var = required_step_field(index, "var", step.var.as_deref())?;
            let mu = required_step_number(index, "mu", step.mu)?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = one_sample_ttest_csv(&rows, &headers, &var, mu, 0.05)?;
            Ok((
                "stats.ttest.one_sample".to_string(),
                json!({
                    "kind": "ttest.one_sample",
                    "var": var,
                    "mu": mu,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::AnovaOneway => {
            let var = required_step_field(index, "var", step.var.as_deref())?;
            let group = required_step_field(index, "group", step.group.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let response = if let Some(block) = &step.block {
                let result = rbd_anova_csv(&rows, &headers, &var, &group, block, NaStrategy::Drop)?;
                serde_json::to_value(result).map_err(stringify_error)?
            } else {
                let result = oneway_anova_csv(&rows, &headers, &var, &group, NaStrategy::Drop)?;
                serde_json::to_value(result).map_err(stringify_error)?
            };
            Ok((
                "stats.anova.oneway".to_string(),
                json!({
                    "kind": "anova.oneway",
                    "var": var,
                    "group": group,
                    "block": &step.block,
                }),
                response,
            ))
        }
        AnalysisKind::NonparamCochranArmitage => {
            let exposure = required_step_field(index, "exposure", step.exposure.as_deref())?;
            let outcome = required_step_field(index, "outcome", step.outcome.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = cochran_armitage_csv(
                &rows,
                &headers,
                &exposure,
                &outcome,
                &step.scores,
                NaStrategy::Drop,
            )?;
            Ok((
                "stats.nonparam.cochran_armitage".to_string(),
                json!({
                    "kind": "nonparam.cochran_armitage",
                    "exposure": exposure,
                    "outcome": outcome,
                    "scores": &step.scores,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::NonparamMcnemar => {
            let var1 = required_step_field(index, "var1", step.var1.as_deref())?;
            let var2 = required_step_field(index, "var2", step.var2.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = mcnemar_csv(&rows, &headers, &var1, &var2, 25, NaStrategy::Drop)?;
            Ok((
                "stats.nonparam.mcnemar".to_string(),
                json!({
                    "kind": "nonparam.mcnemar",
                    "var1": var1,
                    "var2": var2,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::NonparamWilcoxon => {
            let var1 = required_step_field(index, "var1", step.var1.as_deref())?;
            let var2 = required_step_field(index, "var2", step.var2.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = wilcoxon_csv(&rows, &headers, &var1, &var2, NaStrategy::Drop)?;
            Ok((
                "stats.nonparam.wilcoxon".to_string(),
                json!({
                    "kind": "nonparam.wilcoxon",
                    "var1": var1,
                    "var2": var2,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::NonparamMannwhitney => {
            let var = required_step_field(index, "var", step.var.as_deref())?;
            let group = required_step_field(index, "group", step.group.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = mann_whitney_csv(&rows, &headers, &var, &group, NaStrategy::Drop)?;
            Ok((
                "stats.nonparam.mannwhitney".to_string(),
                json!({
                    "kind": "nonparam.mannwhitney",
                    "var": var,
                    "group": group,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::Correlation => {
            let x = required_step_field(index, "x", step.x.as_deref())?;
            let y = required_step_field(index, "y", step.y.as_deref())?;
            let method = step.method.as_deref().unwrap_or("pearson");
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = correlation_csv(&rows, &headers, &x, &y, 0.05, method)?;
            Ok((
                "stats.correlation".to_string(),
                json!({
                    "kind": "correlation",
                    "x": x,
                    "y": y,
                    "method": method,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::EpiOrRr => {
            let exposure = required_step_field(index, "exposure", step.exposure.as_deref())?;
            let outcome = required_step_field(index, "outcome", step.outcome.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = or_rr_csv(
                &rows,
                &headers,
                &exposure,
                &outcome,
                &step.strata,
                step.exposure_event.as_deref(),
                step.outcome_event.as_deref(),
                0.05,
                NaStrategy::Drop,
            )?;
            Ok((
                "stats.epi.or_rr".to_string(),
                json!({
                    "kind": "epi.or_rr",
                    "exposure": exposure,
                    "outcome": outcome,
                    "strata": &step.strata,
                    "exposure_event": &step.exposure_event,
                    "outcome_event": &step.outcome_event,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::EpiStandardize => {
            let event = required_step_field(index, "event", step.event.as_deref())?;
            let person_time =
                required_step_field(index, "person_time", step.person_time.as_deref())?;
            let age_group = required_step_field(index, "age_group", step.age_group.as_deref())?;
            let standard_pop =
                required_step_field(index, "standard_pop", step.standard_pop.as_deref())?;
            let method = step.method.as_deref().unwrap_or("direct");
            let standard_pop = resolve_step_reference(analysis_path, &standard_pop);
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = standardize_csv(
                &rows,
                &headers,
                method,
                &event,
                &person_time,
                &age_group,
                &standard_pop,
                0.05,
                NaStrategy::Drop,
            )?;
            Ok((
                "stats.epi.standardize".to_string(),
                json!({
                    "kind": "epi.standardize",
                    "method": method,
                    "event": event,
                    "person_time": person_time,
                    "age_group": age_group,
                    "standard_pop": standard_pop,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::EpiAttributable => {
            let exposure = required_step_field(index, "exposure", step.exposure.as_deref())?;
            let outcome = required_step_field(index, "outcome", step.outcome.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = attributable_csv(
                &rows,
                &headers,
                &exposure,
                &outcome,
                step.person_time.as_deref(),
                step.exposure_prevalence,
                0.05,
                NaStrategy::Drop,
            )?;
            Ok((
                "stats.epi.attributable".to_string(),
                json!({
                    "kind": "epi.attributable",
                    "exposure": exposure,
                    "outcome": outcome,
                    "person_time": &step.person_time,
                    "exposure_prevalence": step.exposure_prevalence,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::DiagnosticNormality => {
            let var = required_step_field(index, "var", step.var.as_deref())?;
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = normality_csv(&rows, &headers, &var, NaStrategy::Drop)?;
            Ok((
                "stats.diagnostic.normality".to_string(),
                json!({
                    "kind": "diagnostic.normality",
                    "var": var,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::DiagnosticVariance => {
            let var = required_step_field(index, "var", step.var.as_deref())?;
            let group = required_step_field(index, "group", step.group.as_deref())?;
            let center = step.center.as_deref().unwrap_or("median");
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result =
                variance_homogeneity_csv(&rows, &headers, &var, &group, center, NaStrategy::Drop)?;
            Ok((
                "stats.diagnostic.variance".to_string(),
                json!({
                    "kind": "diagnostic.variance",
                    "var": var,
                    "group": group,
                    "center": center,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::SurvivalLifetable => {
            let intervals = required_step_field(index, "intervals", step.intervals.as_deref())?;
            let input_format = step.input_format.as_deref().unwrap_or("grouped");
            let (headers, rows) = read_workflow_rows(data_path)?;
            let result = if input_format.eq_ignore_ascii_case("individual") {
                let time = required_step_field(index, "time", step.time.as_deref())?;
                let status = required_step_field(index, "status", step.status.as_deref())?;
                lifetable_individual_csv(
                    &rows,
                    &headers,
                    &time,
                    &status,
                    &intervals,
                    0.05,
                    NaStrategy::Drop,
                )?
            } else {
                let entering = required_step_field(index, "entering", step.entering.as_deref())?;
                let events = required_step_field(index, "events", step.events.as_deref())?;
                let withdrawals =
                    required_step_field(index, "withdrawals", step.withdrawals.as_deref())?;
                lifetable_csv(
                    &rows,
                    &headers,
                    &intervals,
                    &entering,
                    &events,
                    &withdrawals,
                    0.05,
                    NaStrategy::Drop,
                )?
            };
            Ok((
                "stats.survival.lifetable".to_string(),
                json!({
                    "kind": "survival.lifetable",
                    "input_format": input_format,
                    "intervals": intervals,
                    "time": &step.time,
                    "status": &step.status,
                    "entering": &step.entering,
                    "events": &step.events,
                    "withdrawals": &step.withdrawals,
                }),
                serde_json::to_value(result).map_err(stringify_error)?,
            ))
        }
        AnalysisKind::Model => match step.model {
            Some(ModelKind::Logistic) => {
                let args = ModelLogisticArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    outcome: required_step_field(index, "outcome", step.outcome.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_logistic(&args, engine)?;
                Ok((
                    "model_logistic".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            Some(ModelKind::Cox) => {
                let args = ModelCoxArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    time: required_step_field(index, "time", step.time.as_deref())?,
                    event: required_step_field(index, "event", step.event.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_cox(&args, engine)?;
                Ok((
                    "model_cox".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            Some(ModelKind::Linear) => {
                let args = ModelLinearArgs {
                    data: None,
                    analysis: Some(analysis_path.to_path_buf()),
                    outcome: required_step_field(index, "outcome", step.outcome.as_deref())?,
                    predictors: required_step_list(index, "predictors", &step.predictors)?,
                    adjust: step.adjust.clone(),
                    strata: step.strata.clone(),
                };
                let result = handle_model_linear(&args, engine)?;
                Ok((
                    "model_linear".to_string(),
                    json!(args),
                    serde_json::to_value(result).map_err(stringify_error)?,
                ))
            }
            None => Err(format!(
                "analysis step {index} has kind `model` but no `model` field"
            )),
        },
        // Phase 2 + Phase 3 methods: not yet wired in workflow runner
        _ => Err(format!(
            "analysis step {index} has kind `{:?}` which is not yet wired in the workflow runner",
            step.kind
        )),
    }
}

fn read_workflow_rows(path: &Path) -> Result<(csv::StringRecord, Vec<csv::StringRecord>), String> {
    let mut reader = csv::Reader::from_path(path).map_err(stringify_error)?;
    let headers = reader.headers().map_err(stringify_error)?.clone();
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(stringify_error)?;
    Ok((headers, rows))
}

fn required_step_field(index: usize, field: &str, value: Option<&str>) -> Result<String, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("analysis step {index} requires `{field}`"))
}

fn required_step_number(index: usize, field: &str, value: Option<f64>) -> Result<f64, String> {
    value.ok_or_else(|| format!("analysis step {index} requires `{field}`"))
}

fn required_step_list(index: usize, field: &str, value: &[String]) -> Result<Vec<String>, String> {
    if value.is_empty() {
        Err(format!("analysis step {index} requires `{field}`"))
    } else {
        Ok(value.to_vec())
    }
}

fn resolve_step_reference(analysis_path: &Path, value: &str) -> String {
    let path = Path::new(value);
    if path.is_absolute() {
        return value.to_string();
    }
    let candidate = resolve_relative_to_analysis(analysis_path, path);
    if candidate.is_file() {
        candidate.display().to_string()
    } else {
        value.to_string()
    }
}
