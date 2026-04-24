//! Chat tool definitions and execution dispatch.

use std::path::Path;

use serde_json::{json, Value};

use api::{InputContentBlock, InputMessage, OutputContentBlock, ToolDefinition};

use crate::cli::{
    InspectArgs, ModelCoxArgs, ModelLinearArgs, ModelLogisticArgs, RateArgs, ReportBuildArgs,
    TableOneArgs,
};
use crate::handlers::{
    handle_inspect, handle_model_cox, handle_model_linear, handle_model_logistic, handle_rate,
    handle_tableone,
};
use crate::helpers::stringify_error;
use crate::report::{handle_report_build, persist_run_artifacts};

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
    const PATH_KEYS: &[&str] = &["data_path", "data", "analysis"];
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
            let args: TableOneArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `tableone`: {error}")
                })?;
            let result = handle_tableone(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "rate" => {
            let args: RateArgs = serde_json::from_value(input.clone()).map_err(|error| {
                format!("Invalid input for tool `rate`: {error}")
            })?;
            let result = handle_rate(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_logistic" => {
            let args: ModelLogisticArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_logistic`: {error}")
                })?;
            let result = handle_model_logistic(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_cox" => {
            let args: ModelCoxArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_cox`: {error}")
                })?;
            let result = handle_model_cox(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "model_linear" => {
            let args: ModelLinearArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `model_linear`: {error}")
                })?;
            let result = handle_model_linear(&args, crate::bridge::Engine::Rust)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        "report_build" => {
            let args: ReportBuildArgs =
                serde_json::from_value(input.clone()).map_err(|error| {
                    format!("Invalid input for tool `report_build`: {error}")
                })?;
            let result = handle_report_build(&args)?;
            (
                serde_json::to_value(&args).map_err(stringify_error)?,
                serde_json::to_value(result).map_err(stringify_error)?,
            )
        }
        other => {
            return Err(format!(
                "Unknown tool `{other}`. Available tools are inspect, tableone, rate, model_logistic, model_cox, model_linear, and report_build."
            ))
        }
    };

    if let Some(base_dir) = artifacts_dir {
        persist_run_artifacts(base_dir, name, &request, &response)?;
    }

    serde_json::to_string(&response).map_err(stringify_error)
}
