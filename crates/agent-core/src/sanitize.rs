//! Sensitive field sanitization (R8.6, R13.2).
//!
//! Ensures that `api_key`, Dataset cell content, and raw upload file paths
//! never appear in `Display`, `Debug`, or `serde_json::to_string` output.

use std::fmt;

use crate::models::{ErrorPayload, SkillRun};
use crate::traits::llm_provider::LlmRequest;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Placeholder used when redacting API keys.
const REDACTED_KEY: &str = "***REDACTED***";

/// Placeholder prefix used when redacting file paths.
const REDACTED_PATH_PREFIX: &str = "<dataset:";

// ---------------------------------------------------------------------------
// Sanitize trait
// ---------------------------------------------------------------------------

/// Trait for producing a sanitized copy of a value.
///
/// The returned copy must not contain:
/// - API keys (any string matching common key patterns)
/// - Raw upload file paths (absolute paths)
/// - Dataset cell content (numeric/text data values embedded in args)
pub trait Sanitize {
    /// Returns a sanitized copy of `self` with sensitive fields redacted.
    #[must_use]
    fn sanitize(&self) -> Self;
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Redact an API key, replacing it with `***REDACTED***`.
///
/// If the input is empty, returns the redacted placeholder anyway to avoid
/// leaking information about key presence.
#[must_use]
pub fn redact_api_key(_key: &str) -> String {
    REDACTED_KEY.to_string()
}

/// Redact an absolute file path, replacing it with a dataset-id reference.
///
/// Heuristic: if the path contains a path separator (`/` or `\`) and looks
/// like an absolute path (starts with `/`, `C:\`, `\\`, etc.), replace it
/// with `<dataset:filename>` where `filename` is the last path component.
#[must_use]
pub fn redact_path(path: &str) -> String {
    if is_absolute_path(path) {
        let filename = path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("unknown");
        format!("{REDACTED_PATH_PREFIX}{filename}>")
    } else {
        path.to_string()
    }
}

/// Determine if a string looks like an absolute file path.
fn is_absolute_path(s: &str) -> bool {
    // Unix absolute
    if s.starts_with('/') {
        return true;
    }
    // Windows absolute: C:\ or C:/
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
            return true;
        }
    }
    // UNC path
    if s.starts_with("\\\\") {
        return true;
    }
    false
}

/// Redact sensitive content within a JSON value recursively.
///
/// - String values that look like absolute paths are redacted.
/// - Keys containing `api_key`, `apikey`, `secret`, `token` have their values redacted.
/// - Array/object values containing data cells (numbers/strings in arrays) within
///   keys named `data`, `cells`, `values`, `rows` are replaced with `"[REDACTED_DATA]"`.
#[must_use]
pub fn redact_json_value(value: &serde_json::Value) -> serde_json::Value {
    redact_json_value_inner(value, None)
}

fn redact_json_value_inner(
    value: &serde_json::Value,
    parent_key: Option<&str>,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            // If parent key is a sensitive key name, redact entirely
            if let Some(key) = parent_key {
                if is_sensitive_key(key) {
                    return serde_json::Value::String(REDACTED_KEY.to_string());
                }
                if is_data_key(key) {
                    return serde_json::Value::String("[REDACTED_DATA]".to_string());
                }
            }
            // If the string looks like an absolute path, redact it
            if is_absolute_path(s) {
                serde_json::Value::String(redact_path(s))
            } else {
                value.clone()
            }
        }
        serde_json::Value::Number(_) => {
            // If inside a data key, redact
            if let Some(key) = parent_key {
                if is_data_key(key) {
                    return serde_json::Value::String("[REDACTED_DATA]".to_string());
                }
            }
            value.clone()
        }
        serde_json::Value::Array(arr) => {
            // If parent key is a data key, redact entire array content
            if let Some(key) = parent_key {
                if is_data_key(key) {
                    return serde_json::Value::String("[REDACTED_DATA]".to_string());
                }
            }
            serde_json::Value::Array(
                arr.iter()
                    .map(|v| redact_json_value_inner(v, parent_key))
                    .collect(),
            )
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), redact_json_value_inner(v, Some(k.as_str()))))
                .collect();
            serde_json::Value::Object(new_map)
        }
        _ => value.clone(),
    }
}

/// Check if a key name indicates a sensitive credential field.
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("credential")
}

/// Check if a key name indicates dataset cell data.
fn is_data_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower == "data"
        || lower == "cells"
        || lower == "values"
        || lower == "rows"
        || lower == "cell_content"
        || lower == "raw_data"
}

/// Redact sensitive substrings from a plain text string.
///
/// Scans for patterns that look like API keys or absolute paths and redacts them.
#[must_use]
pub fn redact_string(s: &str, sensitive_patterns: &[&str]) -> String {
    let mut result = s.to_string();
    for pattern in sensitive_patterns {
        if !pattern.is_empty() && result.contains(pattern) {
            result = result.replace(pattern, REDACTED_KEY);
        }
    }
    // Also redact any remaining absolute paths
    result = redact_absolute_paths_in_text(&result);
    result
}

/// Find and redact absolute paths embedded in free text.
fn redact_absolute_paths_in_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut i = 0;
    let bytes = text.as_bytes();

    while i < bytes.len() {
        // Check for Unix absolute path: /something/...
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphanumeric() {
            // Look ahead to find path end
            if let Some((path, end)) = extract_path(text, i) {
                if path.contains('/') && path.len() > 2 {
                    result.push_str(&redact_path(path));
                    i = end;
                    // Re-sync chars iterator
                    chars = text[i..].chars().peekable();
                    continue;
                }
            }
        }
        // Check for Windows absolute path: X:\...
        if i + 2 < bytes.len()
            && bytes[i].is_ascii_alphabetic()
            && bytes[i + 1] == b':'
            && (bytes[i + 2] == b'\\' || bytes[i + 2] == b'/')
        {
            if let Some((path, end)) = extract_path(text, i) {
                result.push_str(&redact_path(path));
                i = end;
                chars = text[i..].chars().peekable();
                continue;
            }
        }

        if let Some(c) = chars.next() {
            result.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }
    result
}

/// Extract a file path starting at position `start` in `text`.
/// Returns the path slice and the end position.
fn extract_path(text: &str, start: usize) -> Option<(&str, usize)> {
    let rest = &text[start..];
    // A path ends at whitespace, quote, or certain punctuation
    let end_offset = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>' || c == '`')
        .unwrap_or(rest.len());
    if end_offset > 1 {
        Some((&rest[..end_offset], start + end_offset))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Sanitize implementations
// ---------------------------------------------------------------------------

impl Sanitize for ErrorPayload {
    fn sanitize(&self) -> Self {
        Self {
            error_code: self.error_code,
            message: redact_absolute_paths_in_text(&self.message),
            details: self.details.as_ref().map(redact_json_value),
        }
    }
}

impl Sanitize for SkillRun {
    fn sanitize(&self) -> Self {
        Self {
            run_id: self.run_id,
            skill_id: self.skill_id.clone(),
            args: redact_json_value(&self.args),
            started_at: self.started_at,
            finished_at: self.finished_at,
            outcome: self.outcome.clone(),
        }
    }
}

impl Sanitize for LlmRequest {
    fn sanitize(&self) -> Self {
        use crate::traits::llm_provider::LlmMessage;

        Self {
            messages: self
                .messages
                .iter()
                .map(|msg| LlmMessage {
                    role: msg.role,
                    content: redact_absolute_paths_in_text(&msg.content),
                })
                .collect(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        }
    }
}

// ---------------------------------------------------------------------------
// SanitizedDisplay wrapper
// ---------------------------------------------------------------------------

/// A wrapper that implements `Display` and `Debug` by sanitizing the inner value first.
///
/// Use this to safely log or display objects that may contain sensitive data.
///
/// # Example
/// ```ignore
/// let payload = ErrorPayload::new(ErrorCode::SkillExecutionFailed, "failed at /home/user/data.csv");
/// println!("{}", SanitizedDisplay(&payload));
/// // Output will have the path redacted
/// ```
pub struct SanitizedDisplay<'a, T: Sanitize + fmt::Debug>(pub &'a T);

impl<T: Sanitize + fmt::Debug> fmt::Display for SanitizedDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sanitized = self.0.sanitize();
        write!(f, "{sanitized:?}")
    }
}

impl<T: Sanitize + fmt::Debug> fmt::Debug for SanitizedDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sanitized = self.0.sanitize();
        write!(f, "{sanitized:?}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::error::ErrorCode;
    use crate::models::skill::{SkillOutcome, SkillRun};
    use crate::traits::llm_provider::{LlmMessage, LlmRequest, LlmRole};
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_redact_api_key() {
        assert_eq!(redact_api_key("sk-abc123xyz"), "***REDACTED***");
        assert_eq!(redact_api_key(""), "***REDACTED***");
    }

    #[test]
    fn test_redact_path_unix() {
        let result = redact_path("/home/user/uploads/data.csv");
        assert_eq!(result, "<dataset:data.csv>");
        assert!(!result.contains("/home/user"));
    }

    #[test]
    fn test_redact_path_windows() {
        let result = redact_path("C:\\Users\\ljx\\Documents\\patient_data.xlsx");
        assert_eq!(result, "<dataset:patient_data.xlsx>");
        assert!(!result.contains("C:\\Users"));
    }

    #[test]
    fn test_redact_path_unc() {
        let result = redact_path("\\\\server\\share\\secret.csv");
        assert_eq!(result, "<dataset:secret.csv>");
    }

    #[test]
    fn test_redact_path_relative_unchanged() {
        let result = redact_path("data.csv");
        assert_eq!(result, "data.csv");

        let result = redact_path("relative/path/file.csv");
        assert_eq!(result, "relative/path/file.csv");
    }

    #[test]
    fn test_is_absolute_path() {
        assert!(is_absolute_path("/home/user/file.csv"));
        assert!(is_absolute_path("C:\\Users\\file.csv"));
        assert!(is_absolute_path("D:/data/file.csv"));
        assert!(is_absolute_path("\\\\server\\share"));
        assert!(!is_absolute_path("relative/path"));
        assert!(!is_absolute_path("file.csv"));
        assert!(!is_absolute_path(""));
    }

    #[test]
    fn test_sanitize_error_payload_redacts_path() {
        let payload = ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            "统计任务失败：文件 /home/user/uploads/secret_data.csv 无法解析",
        );
        let sanitized = payload.sanitize();

        assert!(!sanitized.message.contains("/home/user/uploads/secret_data.csv"));
        assert!(sanitized.message.contains("<dataset:secret_data.csv>"));
        assert_eq!(sanitized.error_code, ErrorCode::SkillExecutionFailed);
    }

    #[test]
    fn test_sanitize_error_payload_redacts_details_with_api_key() {
        let details = serde_json::json!({
            "api_key": "sk-secret-key-12345",
            "endpoint": "https://api.deepseek.com/v1"
        });
        let payload = ErrorPayload::with_details(
            ErrorCode::LlmUnavailable,
            "AI 服务暂时不可用",
            details,
        );
        let sanitized = payload.sanitize();

        let details = sanitized.details.unwrap();
        assert_eq!(details["api_key"], "***REDACTED***");
        assert!(!serde_json::to_string(&details)
            .unwrap()
            .contains("sk-secret-key-12345"));
    }

    #[test]
    fn test_sanitize_skill_run_redacts_data_values() {
        let run = SkillRun {
            run_id: Uuid::new_v4(),
            skill_id: "model_linear".to_string(),
            args: serde_json::json!({
                "outcome": "blood_pressure",
                "data": [1.5, 2.3, 4.7, 8.1],
                "file_path": "/home/user/uploads/patient_records.csv"
            }),
            started_at: Utc::now(),
            finished_at: None,
            outcome: SkillOutcome::Pending,
        };
        let sanitized = run.sanitize();

        let args_str = serde_json::to_string(&sanitized.args).unwrap();
        // Data values should be redacted
        assert!(!args_str.contains("1.5"));
        assert!(!args_str.contains("2.3"));
        // File path should be redacted
        assert!(!args_str.contains("/home/user/uploads/patient_records.csv"));
        // Skill ID preserved
        assert_eq!(sanitized.skill_id, "model_linear");
    }

    #[test]
    fn test_sanitize_llm_request_redacts_paths_in_content() {
        let req = LlmRequest {
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: "You are a statistical assistant.".to_string(),
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: "分析文件 C:\\Users\\ljx\\data\\experiment.csv 中的数据".to_string(),
                },
            ],
            model: "deepseek-chat".to_string(),
            max_tokens: Some(2048),
            temperature: Some(0.7),
        };
        let sanitized = req.sanitize();

        let user_msg = &sanitized.messages[1];
        assert!(!user_msg.content.contains("C:\\Users\\ljx\\data\\experiment.csv"));
        assert!(user_msg.content.contains("<dataset:experiment.csv>"));
        // System message without paths should be unchanged
        assert_eq!(
            sanitized.messages[0].content,
            "You are a statistical assistant."
        );
    }

    #[test]
    fn test_sanitized_display_no_sensitive_data() {
        let payload = ErrorPayload::with_details(
            ErrorCode::SkillExecutionFailed,
            "失败：/var/data/uploads/secret.csv",
            serde_json::json!({"api_key": "sk-12345", "token": "bearer-xyz"}),
        );

        let display_output = format!("{}", SanitizedDisplay(&payload));
        let debug_output = format!("{:?}", SanitizedDisplay(&payload));

        // Neither Display nor Debug should contain sensitive data
        assert!(!display_output.contains("sk-12345"));
        assert!(!display_output.contains("bearer-xyz"));
        assert!(!display_output.contains("/var/data/uploads/secret.csv"));

        assert!(!debug_output.contains("sk-12345"));
        assert!(!debug_output.contains("bearer-xyz"));
        assert!(!debug_output.contains("/var/data/uploads/secret.csv"));
    }

    #[test]
    fn test_sanitized_display_preserves_non_sensitive() {
        let payload = ErrorPayload::new(
            ErrorCode::MessageTooLong,
            "消息过长：当前 9000 字，超过上限 8000 字",
        );
        let display_output = format!("{}", SanitizedDisplay(&payload));

        // Non-sensitive content should be preserved
        assert!(display_output.contains("9000"));
        assert!(display_output.contains("8000"));
        assert!(display_output.contains("MessageTooLong"));
    }

    #[test]
    fn test_redact_json_value_nested_sensitive_keys() {
        let value = serde_json::json!({
            "config": {
                "api_key": "sk-secret",
                "model": "deepseek-chat",
                "nested": {
                    "secret": "my-secret-value",
                    "normal": "visible"
                }
            }
        });
        let redacted = redact_json_value(&value);

        let s = serde_json::to_string(&redacted).unwrap();
        assert!(!s.contains("sk-secret"));
        assert!(!s.contains("my-secret-value"));
        assert!(s.contains("deepseek-chat"));
        assert!(s.contains("visible"));
    }

    #[test]
    fn test_redact_json_value_data_arrays() {
        let value = serde_json::json!({
            "skill_id": "model_linear",
            "rows": [[1.0, 2.0], [3.0, 4.0]],
            "cells": ["patient_a", "patient_b"]
        });
        let redacted = redact_json_value(&value);

        let s = serde_json::to_string(&redacted).unwrap();
        assert!(!s.contains("1.0"));
        assert!(!s.contains("patient_a"));
        assert!(s.contains("model_linear"));
    }

    #[test]
    fn test_serde_json_to_string_no_sensitive_data() {
        let run = SkillRun {
            run_id: Uuid::new_v4(),
            skill_id: "model_cox".to_string(),
            args: serde_json::json!({
                "api_key": "sk-dangerous-key",
                "data": [100.0, 200.0, 300.0],
                "file": "/tmp/uploads/patient.csv"
            }),
            started_at: Utc::now(),
            finished_at: None,
            outcome: SkillOutcome::Pending,
        };
        let sanitized = run.sanitize();
        let json_output = serde_json::to_string(&sanitized).unwrap();

        assert!(!json_output.contains("sk-dangerous-key"));
        assert!(!json_output.contains("100.0"));
        assert!(!json_output.contains("/tmp/uploads/patient.csv"));
    }

    #[test]
    fn test_redact_string_with_known_patterns() {
        let text = "Error calling api with key sk-abc123 at endpoint https://api.example.com";
        let result = redact_string(text, &["sk-abc123"]);
        assert!(!result.contains("sk-abc123"));
        assert!(result.contains("***REDACTED***"));
    }
}
