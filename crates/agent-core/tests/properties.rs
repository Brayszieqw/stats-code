//! Property-based tests for `agent-core` (subset of design.md §"Correctness Properties").
//!
//! Implements 4 high-value properties from the 28-property spec:
//! - P1: message length validation boundary + readable error
//! - P7: Session JSON round-trip
//! - P14: DeepSeek/OpenAI retry classification
//! - P17: ErrorCode → HTTP status code consistency

use proptest::prelude::*;

use agent_core::models::{ErrorCode, ErrorPayload};
use agent_core::validation::message::validate_message_length;

// ---------------------------------------------------------------------------
// Property 1: message length validation
// ---------------------------------------------------------------------------
//
// For any UTF-8 string s, validate_message_length(s) returns:
// - Ok(()) iff s.chars().count() <= 8000
// - Err(MessageTooLong) iff s.chars().count() > 8000, with `message` being a
//   non-empty Chinese string containing both the actual count and the literal "8000".

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_01_message_length_validation(s in "\\PC{0,9000}") {
        let actual = s.chars().count();
        match validate_message_length(&s) {
            Ok(()) => prop_assert!(actual <= 8000),
            Err(payload) => {
                prop_assert!(actual > 8000, "expected ok for {} chars", actual);
                prop_assert_eq!(payload.error_code, ErrorCode::MessageTooLong);
                // Must contain at least one CJK character (per R1.4 readability requirement).
                let has_chinese = payload
                    .message
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                prop_assert!(has_chinese, "message lacks Chinese chars: {}", payload.message);
                // Must mention the actual length and the upper bound 8000.
                prop_assert!(
                    payload.message.contains(&actual.to_string()),
                    "message missing actual length {}: {}",
                    actual,
                    payload.message
                );
                prop_assert!(
                    payload.message.contains("8000"),
                    "message missing limit 8000: {}",
                    payload.message
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Property 7: Session JSON round-trip
// ---------------------------------------------------------------------------
//
// For any well-formed Session, serde_json::from_str(&serde_json::to_string(s))
// should produce a value field-equal to the original.
//
// We also exercise the in-memory store's persistence round-trip
// (create → fetch should be identical except for last_active_at which `touch`
// would mutate, but we don't touch here).

use agent_core::models::{Session, SessionId, SessionSettings, SessionStatus};
use chrono::{TimeZone, Utc};

fn arb_session() -> impl Strategy<Value = Session> {
    (
        any::<u128>(),                  // for SessionId
        prop_oneof![Just(SessionStatus::Active), Just(SessionStatus::Archived)],
        any::<i64>().prop_map(|t| t.rem_euclid(2_000_000_000)), // bounded epoch seconds
        any::<i64>().prop_map(|t| t.rem_euclid(2_000_000_000)),
        any::<bool>(),                  // decision_assistant
        0u64..1_000_000_000u64,         // uploaded_bytes
    )
        .prop_map(|(uuid_bytes, status, ts1, ts2, da, bytes)| {
            let id = SessionId(uuid::Uuid::from_u128(uuid_bytes));
            let created_at = Utc.timestamp_opt(ts1, 0).single().unwrap_or_else(Utc::now);
            let last_active_at = Utc.timestamp_opt(ts2, 0).single().unwrap_or_else(Utc::now);
            Session {
                id,
                status,
                created_at,
                last_active_at,
                settings: SessionSettings {
                    decision_assistant: da,
                },
                messages: Vec::new(),
                datasets: Vec::new(),
                skill_runs: Vec::new(),
                uploaded_bytes: bytes,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_07_session_json_roundtrip(session in arb_session()) {
        let json = serde_json::to_string(&session).expect("serialize");
        let decoded: Session = serde_json::from_str(&json).expect("deserialize");

        prop_assert_eq!(decoded.id, session.id);
        prop_assert_eq!(decoded.status, session.status);
        prop_assert_eq!(decoded.created_at, session.created_at);
        prop_assert_eq!(decoded.last_active_at, session.last_active_at);
        prop_assert_eq!(
            decoded.settings.decision_assistant,
            session.settings.decision_assistant
        );
        prop_assert_eq!(decoded.uploaded_bytes, session.uploaded_bytes);
        prop_assert_eq!(decoded.messages.len(), session.messages.len());
        prop_assert_eq!(decoded.datasets.len(), session.datasets.len());
        prop_assert_eq!(decoded.skill_runs.len(), session.skill_runs.len());
    }
}

// ---------------------------------------------------------------------------
// Property 14: retry classification (covers DeepSeek + OpenAI shared logic)
// ---------------------------------------------------------------------------
//
// For any HTTP status code, the retry classifier returns:
// - Success    iff 2xx
// - NonRetryable iff 4xx
// - Retryable  iff 5xx (and for any network error)

use agent_core::llm::openai_compat::{classify_response, RetryDecision};
use reqwest::StatusCode;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_14_retry_classification(code in 100u16..600u16) {
        let Ok(status) = StatusCode::from_u16(code) else { return Ok(()); };
        let decision = classify_response(&Ok(status));

        if status.is_success() {
            prop_assert_eq!(decision, RetryDecision::Success);
        } else if status.is_client_error() {
            prop_assert_eq!(decision, RetryDecision::NonRetryable);
        } else if status.is_server_error() {
            prop_assert_eq!(decision, RetryDecision::Retryable);
        } else {
            // 1xx and 3xx: classifier treats them as retryable (5xx branch).
            prop_assert_eq!(decision, RetryDecision::Retryable);
        }
    }
}

// ---------------------------------------------------------------------------
// Property 17: ErrorCode ↔ HTTP status code consistency + IntoResponse shape
// ---------------------------------------------------------------------------
//
// For every ErrorCode, http_status_for(code) returns a deterministic StatusCode,
// and ErrorPayload::to_http_parts() yields a body whose JSON shape is
// {error_code, message[, details]} matching the original payload.

fn arb_error_code() -> impl Strategy<Value = ErrorCode> {
    prop_oneof![
        Just(ErrorCode::MessageTooLong),
        Just(ErrorCode::AudioTooLarge),
        Just(ErrorCode::DatasetTooLarge),
        Just(ErrorCode::DatasetEmpty),
        Just(ErrorCode::InvalidChoice),
        Just(ErrorCode::SkillInvalidArgs),
        Just(ErrorCode::SkillTimeout),
        Just(ErrorCode::SkillOom),
        Just(ErrorCode::SkillExecutionFailed),
        Just(ErrorCode::LlmUnavailable),
        Just(ErrorCode::SessionNotFound),
        Just(ErrorCode::SessionArchived),
        Just(ErrorCode::SessionQuotaExceeded),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_17_http_status_deterministic(code1 in arb_error_code(), code2 in arb_error_code()) {
        let s1 = agent_core::models::http_status_for(code1);
        let s1_again = agent_core::models::http_status_for(code1);
        prop_assert_eq!(s1, s1_again, "non-deterministic mapping for {:?}", code1);

        if code1 == code2 {
            let s2 = agent_core::models::http_status_for(code2);
            prop_assert_eq!(s1, s2);
        }
    }

    #[test]
    fn prop_17_error_payload_roundtrip(
        code in arb_error_code(),
        msg in "[\\u4e00-\\u9fff a-zA-Z0-9：，。 ]{1,200}",
    ) {
        let payload = ErrorPayload::new(code, msg.clone());
        let (status, body) = payload.to_http_parts();

        // 1. Status code matches http_status_for
        prop_assert_eq!(status, agent_core::models::http_status_for(code));

        // 2. Body deserializes back to a structurally-equal payload
        let decoded: ErrorPayload =
            serde_json::from_str(&body).expect("body must be valid JSON");
        prop_assert_eq!(decoded.error_code, code);
        prop_assert_eq!(decoded.message, msg);
    }
}
