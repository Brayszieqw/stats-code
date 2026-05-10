use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::bridge::Engine;
use crate::cli::{
    InspectArgs, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, RateArgs, ReportVerifyArgs,
    TableOneArgs, WorkflowRunArgs,
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
    }
}

fn required_step_field(index: usize, field: &str, value: Option<&str>) -> Result<String, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("analysis step {index} requires `{field}`"))
}

fn required_step_list(index: usize, field: &str, value: &[String]) -> Result<Vec<String>, String> {
    if value.is_empty() {
        Err(format!("analysis step {index} requires `{field}`"))
    } else {
        Ok(value.to_vec())
    }
}
