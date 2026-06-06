//! Error types and error code definitions.

use serde::{Deserialize, Serialize};

/// Unified error response payload (R9.3).
///
/// All API error responses use this structure:
/// `{ "error_code": string, "message": string, "details"?: object }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub error_code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Enumeration of all application error codes.
///
/// Each variant maps to a specific HTTP status code via [`http_status_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    /// Text message exceeds 8000 character limit (R1.4).
    MessageTooLong,
    /// Audio exceeds 60s duration or 10 MB size (R2.5).
    AudioTooLarge,
    /// Dataset file exceeds 50 MB or 1M rows (R3.4).
    DatasetTooLarge,
    /// Dataset has 0 columns or all rows empty (R3.5).
    DatasetEmpty,
    /// Choice answer not in option list and custom text not allowed (R4.5).
    InvalidChoice,
    /// Skill arguments do not match input schema (R6.5).
    SkillInvalidArgs,
    /// Skill execution exceeded 60s wall time (R10.3).
    SkillTimeout,
    /// Skill execution exceeded memory limit (R10.3).
    SkillOom,
    /// Skill process returned non-zero exit or unparseable output (R10.4).
    SkillExecutionFailed,
    /// `DeepSeek` API unavailable after retries (R8.4).
    LlmUnavailable,
    /// Session ID does not exist (R9.6).
    SessionNotFound,
    /// Session is archived; write operations rejected (R11.4).
    SessionArchived,
    /// Per-session upload quota (200 MB) exceeded (R13.4).
    SessionQuotaExceeded,
}

impl ErrorPayload {
    /// Create a new error payload with the given code and message.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            error_code: code,
            message: message.into(),
            details: None,
        }
    }

    /// Create a new error payload with the given code, message, and structured details.
    #[must_use]
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            error_code: code,
            message: message.into(),
            details: Some(details),
        }
    }

    /// Returns the HTTP status code (as `u16`) for this error payload.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        http_status_for(self.error_code)
    }

    /// Serialize this payload to a JSON byte vector suitable for an HTTP response body.
    #[must_use]
    pub fn to_json_bytes(&self) -> Vec<u8> {
        // ErrorPayload is always serializable; unwrap is safe here.
        serde_json::to_vec(self).expect("ErrorPayload serialization should never fail")
    }

    /// Returns the HTTP status code and JSON body string as a tuple.
    ///
    /// This is a framework-agnostic helper that `agent-server` (or any HTTP layer)
    /// can use to build a response with the correct status code, JSON body, and
    /// `Content-Type: application/json` header.
    #[must_use]
    pub fn to_http_parts(&self) -> (u16, String) {
        let status = http_status_for(self.error_code);
        let body =
            serde_json::to_string(self).expect("ErrorPayload serialization should never fail");
        (status, body)
    }
}

/// Maps an [`ErrorCode`] to its corresponding HTTP status code.
///
/// This is the single source of truth for error-to-status mapping (Property 17).
///
/// | `ErrorCode` | HTTP Status |
/// |-----------|-------------|
/// | `MessageTooLong` | 413 |
/// | `AudioTooLarge` | 413 |
/// | `DatasetTooLarge` | 413 |
/// | `DatasetEmpty` | 422 |
/// | `InvalidChoice` | 422 |
/// | `SkillInvalidArgs` | 422 |
/// | `SkillTimeout` | 504 |
/// | `SkillOom` | 507 |
/// | `SkillExecutionFailed` | 500 |
/// | `LlmUnavailable` | 502 |
/// | `SessionNotFound` | 404 |
/// | `SessionArchived` | 409 |
/// | `SessionQuotaExceeded` | 413 |
#[must_use]
pub fn http_status_for(code: ErrorCode) -> u16 {
    match code {
        ErrorCode::MessageTooLong
        | ErrorCode::AudioTooLarge
        | ErrorCode::DatasetTooLarge
        | ErrorCode::SessionQuotaExceeded => 413,
        ErrorCode::DatasetEmpty | ErrorCode::InvalidChoice | ErrorCode::SkillInvalidArgs => 422,
        ErrorCode::SkillTimeout => 504,
        ErrorCode::SkillOom => 507,
        ErrorCode::SkillExecutionFailed => 500,
        ErrorCode::LlmUnavailable => 502,
        ErrorCode::SessionNotFound => 404,
        ErrorCode::SessionArchived => 409,
    }
}

/// Complete list of all [`ErrorCode`] variants, useful for exhaustive testing.
pub const ALL_ERROR_CODES: &[ErrorCode] = &[
    ErrorCode::MessageTooLong,
    ErrorCode::AudioTooLarge,
    ErrorCode::DatasetTooLarge,
    ErrorCode::DatasetEmpty,
    ErrorCode::InvalidChoice,
    ErrorCode::SkillInvalidArgs,
    ErrorCode::SkillTimeout,
    ErrorCode::SkillOom,
    ErrorCode::SkillExecutionFailed,
    ErrorCode::LlmUnavailable,
    ErrorCode::SessionNotFound,
    ErrorCode::SessionArchived,
    ErrorCode::SessionQuotaExceeded,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify each `ErrorCode` maps to the expected HTTP status code.
    #[test]
    fn test_http_status_for_all_codes() {
        let expected: &[(ErrorCode, u16)] = &[
            (ErrorCode::MessageTooLong, 413),
            (ErrorCode::AudioTooLarge, 413),
            (ErrorCode::DatasetTooLarge, 413),
            (ErrorCode::SessionQuotaExceeded, 413),
            (ErrorCode::DatasetEmpty, 422),
            (ErrorCode::InvalidChoice, 422),
            (ErrorCode::SkillInvalidArgs, 422),
            (ErrorCode::SkillTimeout, 504),
            (ErrorCode::SkillOom, 507),
            (ErrorCode::SkillExecutionFailed, 500),
            (ErrorCode::LlmUnavailable, 502),
            (ErrorCode::SessionNotFound, 404),
            (ErrorCode::SessionArchived, 409),
        ];

        for &(code, status) in expected {
            assert_eq!(
                http_status_for(code),
                status,
                "ErrorCode::{code:?} should map to HTTP {status}"
            );
        }
    }

    /// Verify that `http_status_for` is consistent (same input → same output).
    #[test]
    fn test_http_status_for_consistency() {
        for &code in ALL_ERROR_CODES {
            let first = http_status_for(code);
            let second = http_status_for(code);
            assert_eq!(first, second, "http_status_for must be deterministic");
        }
    }

    /// Verify `ErrorPayload::status_code()` delegates correctly.
    #[test]
    fn test_error_payload_status_code() {
        let payload = ErrorPayload::new(ErrorCode::SessionNotFound, "会话不存在");
        assert_eq!(payload.status_code(), 404);
    }

    /// Verify `to_http_parts` returns correct status and valid JSON body.
    #[test]
    fn test_to_http_parts_basic() {
        let payload = ErrorPayload::new(ErrorCode::MessageTooLong, "消息过长：当前 9000 字，超过上限 8000 字");
        let (status, body) = payload.to_http_parts();

        assert_eq!(status, 413);

        // Body must be valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("body should be valid JSON");
        assert_eq!(parsed["error_code"], "MessageTooLong");
        assert_eq!(parsed["message"], "消息过长：当前 9000 字，超过上限 8000 字");
        // details should be absent (null in JSON means the field is skipped)
        assert!(parsed.get("details").is_none() || parsed["details"].is_null());
    }

    /// Verify `to_http_parts` with details field present.
    #[test]
    fn test_to_http_parts_with_details() {
        let details = serde_json::json!({
            "field_errors": [{"path": "/outcome", "reason": "missing"}]
        });
        let payload = ErrorPayload::with_details(
            ErrorCode::SkillInvalidArgs,
            "参数错误：outcome: missing",
            details.clone(),
        );
        let (status, body) = payload.to_http_parts();

        assert_eq!(status, 422);

        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("body should be valid JSON");
        assert_eq!(parsed["error_code"], "SkillInvalidArgs");
        assert_eq!(parsed["details"], details);
    }

    /// Verify `to_json_bytes` produces the same content as `to_http_parts` body.
    #[test]
    fn test_to_json_bytes_consistency() {
        let payload = ErrorPayload::new(ErrorCode::LlmUnavailable, "AI 服务暂时不可用");
        let (_, body_str) = payload.to_http_parts();
        let body_bytes = payload.to_json_bytes();

        assert_eq!(body_str.as_bytes(), body_bytes.as_slice());
    }

    /// Verify JSON round-trip: serialize then deserialize produces equivalent payload.
    #[test]
    fn test_json_roundtrip() {
        let original = ErrorPayload::with_details(
            ErrorCode::DatasetEmpty,
            "数据文件为空：列数为 0",
            serde_json::json!({"reason": "zero_columns"}),
        );

        let json = serde_json::to_string(&original).unwrap();
        let restored: ErrorPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.error_code, original.error_code);
        assert_eq!(restored.message, original.message);
        assert_eq!(restored.details, original.details);
    }

    /// Verify that `ALL_ERROR_CODES` covers every variant (compile-time exhaustiveness
    /// is guaranteed by the match in `http_status_for`, but this checks the constant).
    #[test]
    fn test_all_error_codes_complete() {
        assert_eq!(ALL_ERROR_CODES.len(), 13);
        // Each code should produce a valid HTTP status (100-599 range)
        for &code in ALL_ERROR_CODES {
            let status = http_status_for(code);
            assert!(
                (100..=599).contains(&status),
                "ErrorCode::{code:?} maps to invalid HTTP status {status}"
            );
        }
    }
}
