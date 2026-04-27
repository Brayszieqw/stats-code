//! Chat tool definitions and execution dispatch.

use std::path::Path;

use serde_json::{json, Value};

use api::{InputContentBlock, InputMessage, OutputContentBlock, ToolDefinition};

use crate::cli::{
    InspectArgs, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, RateArgs, ReportBuildArgs,
    TableOneArgs, WorkflowRunArgs,
};
use crate::handlers::{
    handle_inspect, handle_model_cox, handle_model_linear, handle_model_logistic, handle_rate,
    handle_tableone, handle_workflow_run,
};
use crate::helpers::stringify_error;
use crate::report::{handle_report_build, persist_run_artifacts_with_metadata};
use crate::schema::ArtifactMetadata;

use super::PendingToolUse;

pub(crate) fn chat_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "inspect".to_string(),
            description: Some(
                "Inspect a local dataset file and summarize columns, missingness, and inferred variable kinds."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data_path": {
                        "type": "string",
                        "description": "Path to the local dataset file, usually CSV, XLSX, Parquet, or XPT."
                    }
                },
                "required": ["data_path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "tableone".to_string(),
            description: Some(
                "Build a grouped baseline Table 1 from a local dataset or analysis.yaml."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": { "type": "string", "description": "Optional direct dataset path." },
                    "analysis": { "type": "string", "description": "Optional analysis.yaml path." },
                    "by": { "type": "string", "description": "Grouping variable for the baseline table." },
                    "vars": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of variables to include."
                    }
                },
                "required": ["by"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "rate".to_string(),
            description: Some(
                "Compute person-time rates using an event indicator and person-time variable."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": { "type": "string", "description": "Optional direct dataset path." },
                    "analysis": { "type": "string", "description": "Optional analysis.yaml path." },
                    "event": { "type": "string", "description": "Event indicator column." },
                    "person_time": { "type": "string", "description": "Person-time column." },
                    "strata": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional stratification columns."
                    }
                },
                "required": ["event", "person_time"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "model_logistic".to_string(),
            description: Some(
                "Fit a binary-outcome logistic regression model from a dataset or analysis.yaml."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": { "type": "string", "description": "Optional direct dataset path." },
                    "analysis": { "type": "string", "description": "Optional analysis.yaml path." },
                    "outcome": { "type": "string", "description": "Binary outcome variable." },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Primary predictor variables."
                    },
                    "adjust": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional adjustment variables."
                    },
                    "strata": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional stratification variables."
                    }
                },
                "required": ["outcome", "predictors"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "model_cox".to_string(),
            description: Some(
                "Fit a Cox proportional hazards model using time-to-event data."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": { "type": "string", "description": "Optional direct dataset path." },
                    "analysis": { "type": "string", "description": "Optional analysis.yaml path." },
                    "time": { "type": "string", "description": "Follow-up time variable." },
                    "event": { "type": "string", "description": "Event indicator variable." },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Primary predictor variables."
                    },
                    "adjust": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional adjustment variables."
                    },
                    "strata": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional stratification variables."
                    }
                },
                "required": ["time", "event", "predictors"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "model_linear".to_string(),
            description: Some(
                "Fit a linear regression (OLS) model for a continuous outcome from a dataset or analysis.yaml."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "data": { "type": "string", "description": "Optional direct dataset path." },
                    "analysis": { "type": "string", "description": "Optional analysis.yaml path." },
                    "outcome": { "type": "string", "description": "Continuous outcome variable." },
                    "predictors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Primary predictor variables."
                    },
                    "adjust": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional adjustment variables."
                    },
                    "strata": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional stratification variables."
                    }
                },
                "required": ["outcome", "predictors"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "report_build".to_string(),
            description: Some(
                "Build a report bundle from analysis.yaml and optionally fold in saved artifacts."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "analysis": { "type": "string", "description": "Path to analysis.yaml." },
                    "out": { "type": "string", "description": "Optional output directory." },
                    "artifacts": {
                        "type": "string",
                        "description": "Optional artifacts directory containing saved run outputs."
                    },
                    "include_exploratory": {
                        "type": "boolean",
                        "description": "When true, report build may consume exploratory chat/manual artifacts."
                    }
                },
                "required": ["analysis"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "workflow_run".to_string(),
            description: Some(
                "Run the declared analysis.yaml workflow deterministically and then build the report."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "analysis": { "type": "string", "description": "Path to analysis.yaml." },
                    "out": { "type": "string", "description": "Formal workflow artifact/report directory." },
                    "explore_out": { "type": "string", "description": "Optional separate exploratory artifact directory." },
                    "include_exploratory": {
                        "type": "boolean",
                        "description": "When true, report build may consume explicitly supplied exploratory artifacts."
                    },
                    "no_chat": {
                        "type": "boolean",
                        "description": "Keep execution CLI-only; accepted for parity with the CLI flag."
                    }
                },
                "required": ["analysis"],
                "additionalProperties": false
            }),
        },
    ]
}

pub(crate) fn assistant_message_from_response(content: &[OutputContentBlock]) -> InputMessage {
    let content = content
        .iter()
        .filter_map(|block| match block {
            OutputContentBlock::Text { text } => {
                Some(InputContentBlock::Text { text: text.clone() })
            }
            OutputContentBlock::ToolUse { id, name, input } => Some(InputContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            }),
            OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {
                None
            }
        })
        .collect::<Vec<_>>();
    InputMessage {
        role: "assistant".to_string(),
        content,
    }
}

pub(crate) fn collect_pending_tool_uses(content: &[OutputContentBlock]) -> Vec<PendingToolUse> {
    content
        .iter()
        .filter_map(|block| match block {
            OutputContentBlock::ToolUse { id, name, input } => Some(PendingToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// P1 UX4: Short version — shows only filenames for path args, keeps other args intact.
pub(crate) fn summarize_tool_input_short(input: &Value) -> String {
    let Some(object) = input.as_object() else {
        return String::new();
    };
    let mut parts = Vec::new();
    const PATH_KEYS: &[&str] = &[
        "data_path",
        "data",
        "analysis",
        "out",
        "artifacts",
        "explore_out",
    ];
    const OTHER_KEYS: &[&str] = &["by", "event", "time", "outcome"];
    for &key in PATH_KEYS {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            let short = std::path::Path::new(value)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(value);
            parts.push(short.to_string());
        }
    }
    for &key in OTHER_KEYS {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            parts.push(format!("{key}={value}"));
        }
    }
    if let Some(values) = object.get("predictors").and_then(Value::as_array) {
        let joined = values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        if !joined.is_empty() {
            parts.push(format!("predict=[{joined}]"));
        }
    }
    parts.join(", ")
}

pub(crate) fn execute_chat_tool(
    name: &str,
    input: &Value,
    artifacts_dir: Option<&Path>,
) -> Result<String, String> {
    let (request, response) = match name {
        "inspect" => {
            let args: InspectArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `inspect`: {error}")
                })?;
            let result = handle_inspect(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "tableone" => {
            let mut args: TableOneArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `tableone`: {error}")
                })?;
            normalize_data_analysis_paths(&mut args.data, &mut args.analysis);
            let result = handle_tableone(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "rate" => {
            let mut args: RateArgs = serde_json::from_value(input.clone()).map_err(|error| {
                format!("Invalid input for tool `rate`: {error}")
            })?;
            normalize_data_analysis_paths(&mut args.data, &mut args.analysis);
            let result = handle_rate(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_logistic" => {
            let mut args: ModelLogisticArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_logistic`: {error}")
                })?;
            normalize_data_analysis_paths(&mut args.data, &mut args.analysis);
            let result = handle_model_logistic(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_cox" => {
            let mut args: ModelCoxArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_cox`: {error}")
                })?;
            normalize_data_analysis_paths(&mut args.data, &mut args.analysis);
            let result = handle_model_cox(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_linear" => {
            let mut args: ModelLinearArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_linear`: {error}")
                })?;
            normalize_data_analysis_paths(&mut args.data, &mut args.analysis);
            let result = handle_model_linear(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "report_build" => {
            let mut args: ReportBuildArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `report_build`: {error}")
                })?;
            normalize_optional_path(&mut args.out);
            normalize_optional_path(&mut args.artifacts);
            if args.artifacts.is_none() {
                args.artifacts = artifacts_dir.map(Path::to_path_buf);
            }
            let result = handle_report_build(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "workflow_run" => {
            let mut args: WorkflowRunArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `workflow_run`: {error}")
                })?;
            normalize_optional_path(&mut args.out);
            normalize_optional_path(&mut args.explore_out);
            if args.out.is_none() {
                args.out = artifacts_dir.map(Path::to_path_buf);
            }
            let result = handle_workflow_run(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        other => {
            return Err(format!(
                "Unknown tool `{other}`. Available tools are inspect, tableone, rate, model_logistic, model_cox, model_linear, report_build, and workflow_run."
            ))
        }
    };

    if let Some(base_dir) = artifacts_dir {
        let artifact = if name == "workflow_run" {
            None
        } else {
            Some(ArtifactMetadata::exploratory())
        };
        persist_run_artifacts_with_metadata(
            base_dir,
            name,
            &request,
            &response,
            artifact.as_ref(),
        )?;
    }

    serde_json::to_string(&response).map_err(stringify_error)
}

fn normalize_optional_path(path: &mut Option<std::path::PathBuf>) {
    if path
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
    {
        *path = None;
    }
}

fn normalize_data_analysis_paths(
    data: &mut Option<std::path::PathBuf>,
    analysis: &mut Option<std::path::PathBuf>,
) {
    normalize_optional_path(data);
    normalize_optional_path(analysis);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;

    use crate::helpers::{fingerprint_file, resolve_path_for_match};

    use super::execute_chat_tool;

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("stats-code-chat-{label}-{nanos}"))
    }

    #[test]
    fn report_build_tool_defaults_to_chat_artifacts_dir() {
        let root = temp_dir("report-build-artifacts");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        let data_path = root.join("demo.csv");
        fs::write(
            &analysis_path,
            r"
study:
  title: Chat report build
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
        fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");

        let artifacts_dir = root.join("chat-artifacts");
        let tableone_dir = artifacts_dir.join("tableone-1");
        fs::create_dir_all(&tableone_dir).expect("create tableone dir");
        fs::write(
            tableone_dir.join("command.json"),
            r#"{"command":"tableone","request":{}}"#,
        )
        .expect("write command");
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

        let out_dir = root.join("report-output");
        let response = execute_chat_tool(
            "report_build",
            &json!({
                "analysis": analysis_path,
                "artifacts": "",
                "out": out_dir,
            }),
            Some(&artifacts_dir),
        )
        .expect("report_build tool should succeed");

        assert!(response.contains("Consumed 1 analysis result artifact"));
        let report_md =
            fs::read_to_string(root.join("report-output").join("report").join("report.md"))
                .expect("read report");
        assert!(report_md.contains("Table 1 available for `category`"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn tableone_tool_ignores_empty_optional_analysis_path() {
        let root = temp_dir("tableone-empty-analysis");
        fs::create_dir_all(&root).expect("create root");
        let data_path = root.join("demo.csv");
        fs::write(&data_path, "category,data_value\nA,1.0\nB,2.0\n").expect("write csv");

        let response = execute_chat_tool(
            "tableone",
            &json!({
                "data": data_path,
                "analysis": "",
                "by": "category",
                "vars": ["data_value"],
            }),
            None,
        )
        .expect("tableone tool should ignore an empty optional analysis path");

        assert!(response.contains("\"by\":\"category\""));
        assert!(response.contains("\"status\":\"ok\""));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn workflow_run_tool_defaults_to_chat_artifacts_dir() {
        let root = temp_dir("workflow-run-artifacts");
        fs::create_dir_all(&root).expect("create root");
        let analysis_path = root.join("analysis.yaml");
        fs::write(
            &analysis_path,
            r"
schema_version: stats-code.v0
study:
  title: Chat workflow run
  design: cross-sectional
study_context:
  estimand: Descriptive category comparison
  exposure: Category
  comparator: Other category
  outcome: Data value
  missing_data_strategy: Complete-case descriptive summaries
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
  - id: inspect_main
    kind: inspect
  - id: table1_main
    kind: table_one
    by: category
report:
  out_dir: formal-artifacts
  include_methods: true
  include_tables: true
  include_assumptions: true
",
        )
        .expect("write analysis yaml");
        fs::write(
            root.join("demo.csv"),
            "category,data_value\nA,1.0\nB,2.0\nA,3.0\nB,4.0\n",
        )
        .expect("write csv");

        let artifacts_dir = root.join("chat-artifacts");
        let response = execute_chat_tool(
            "workflow_run",
            &json!({
                "analysis": analysis_path,
                "out": "",
                "no_chat": true,
            }),
            Some(&artifacts_dir),
        )
        .expect("workflow_run tool should succeed");

        assert!(response.contains("\"status\":\"ok\""));
        assert!(response.contains("\"run_id\""));
        assert!(artifacts_dir.join("report").join("report.md").is_file());
        assert!(artifacts_dir.join("tables").join("tableone.md").is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }
}
