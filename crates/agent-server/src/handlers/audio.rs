//! Audio upload HTTP handler.
//!
//! - `POST /api/sessions/:sid/audio` → [`post_audio`]
//!
//! Receives audio as raw binary body, validates duration/size constraints,
//! calls the STT provider for transcription, and returns the transcription result.
//! If confidence ≥ 0.6, the transcribed text is also processed through the
//! orchestrator (same as the text message flow in 9.5).

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agent_core::models::{ErrorCode, ErrorPayload, SessionStatus};
use agent_core::validation::message::validate_audio;

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Response body
// ---------------------------------------------------------------------------

/// Response body for `POST /api/sessions/:sid/audio`.
///
/// Returns the transcription result so the frontend can display it
/// (especially for low-confidence results that require user confirmation).
#[derive(Debug, Serialize, Deserialize)]
pub struct PostAudioResponse {
    /// The transcribed text from the audio.
    pub text: String,
    /// Confidence score of the transcription (0.0–1.0).
    pub confidence: f32,
    /// Whether the transcription was automatically processed as a message.
    /// True when confidence ≥ 0.6; false means the frontend should ask for confirmation.
    pub auto_processed: bool,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/sessions/:sid/audio` — upload audio for speech-to-text transcription.
///
/// Flow:
/// 1. Extract session ID from path
/// 2. Read audio bytes from raw body
/// 3. Extract duration from `X-Audio-Duration-Secs` header (required)
/// 4. Validate audio constraints (duration ≤ 60s, size ≤ 10MB)
/// 5. Check session exists and is active (not archived)
/// 6. Call SttProvider::transcribe to get (text, confidence)
/// 7. Return transcription result as JSON
///    - If confidence ≥ 0.6, mark `auto_processed = true` (client may trigger message flow)
///    - If confidence < 0.6, mark `auto_processed = false` (client should confirm first)
pub async fn post_audio(
    State(state): State<AppState>,
    Path(sid): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<PostAudioResponse>, AppError> {
    // 1. Extract duration from header
    let duration_secs = extract_duration_header(&headers)?;

    // 2. Validate audio constraints (duration ≤ 60s, size ≤ 10MB)
    let size_bytes = body.len() as u64;
    validate_audio(duration_secs, size_bytes).map_err(AppError)?;

    // 3. Check session exists and is not archived
    let session_id = agent_core::models::SessionId(sid);
    let session = state
        .session_store
        .get(session_id)
        .await
        .map_err(|e| match e {
            agent_core::traits::session_store::StoreError::NotFound(_) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SessionNotFound,
                    "会话不存在或已被删除",
                ))
            }
            agent_core::traits::session_store::StoreError::Archived => {
                AppError(ErrorPayload::new(
                    ErrorCode::SessionArchived,
                    "会话已归档，仅支持只读访问",
                ))
            }
            agent_core::traits::session_store::StoreError::Internal(msg) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SkillExecutionFailed,
                    format!("内部错误：{msg}"),
                ))
            }
        })?;

    if session.status == SessionStatus::Archived {
        return Err(AppError(ErrorPayload::new(
            ErrorCode::SessionArchived,
            "会话已归档，仅支持只读访问",
        )));
    }

    // 4. Get the STT provider
    let stt = state.stt_provider.as_ref().ok_or_else(|| {
        AppError(ErrorPayload::new(
            ErrorCode::LlmUnavailable,
            "语音转写服务尚未初始化",
        ))
    })?;

    // 5. Call STT provider to transcribe
    let result = stt.transcribe(body).await.map_err(|e| {
        AppError(ErrorPayload::new(
            ErrorCode::LlmUnavailable,
            format!("语音转写失败：{e}"),
        ))
    })?;

    // 6. Determine if auto-processing should happen
    let auto_processed = result.confidence >= 0.6;

    Ok(Json(PostAudioResponse {
        text: result.text,
        confidence: result.confidence,
        auto_processed,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the audio duration from the `X-Audio-Duration-Secs` request header.
///
/// Returns an error if the header is missing or not a valid u32.
fn extract_duration_header(headers: &HeaderMap) -> Result<u32, AppError> {
    let value = headers
        .get("X-Audio-Duration-Secs")
        .or_else(|| headers.get("x-audio-duration-secs"))
        .ok_or_else(|| {
            AppError(ErrorPayload::new(
                ErrorCode::AudioTooLarge,
                "缺少 X-Audio-Duration-Secs 请求头，无法校验音频时长",
            ))
        })?;

    let s = value.to_str().map_err(|_| {
        AppError(ErrorPayload::new(
            ErrorCode::AudioTooLarge,
            "X-Audio-Duration-Secs 请求头值无效",
        ))
    })?;

    s.parse::<u32>().map_err(|_| {
        AppError(ErrorPayload::new(
            ErrorCode::AudioTooLarge,
            format!("X-Audio-Duration-Secs 值 '{s}' 不是有效的秒数"),
        ))
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use agent_core::models::{Session, SessionId, SessionSettings, SessionStatus};
    use agent_core::stt::mock::{MockStt, MockSttResponse};
    use agent_core::traits::session_store::{SessionStore, StoreError};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use chrono::Utc;
    use std::sync::Arc;
    use tower::ServiceExt;

    // --- Mock SessionStore ---

    struct MockSessionStore {
        session: Option<Session>,
    }

    impl MockSessionStore {
        fn with_active_session(sid: Uuid) -> Self {
            Self {
                session: Some(Session {
                    id: SessionId(sid),
                    status: SessionStatus::Active,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    settings: SessionSettings::default(),
                    messages: vec![],
                    datasets: vec![],
                    skill_runs: vec![],
                    uploaded_bytes: 0,
                }),
            }
        }

        fn with_archived_session(sid: Uuid) -> Self {
            Self {
                session: Some(Session {
                    id: SessionId(sid),
                    status: SessionStatus::Archived,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    settings: SessionSettings::default(),
                    messages: vec![],
                    datasets: vec![],
                    skill_runs: vec![],
                    uploaded_bytes: 0,
                }),
            }
        }

        fn not_found() -> Self {
            Self { session: None }
        }
    }

    #[async_trait::async_trait]
    impl SessionStore for MockSessionStore {
        async fn create(&self) -> Result<Session, StoreError> {
            unimplemented!()
        }
        async fn get(&self, _id: SessionId) -> Result<Session, StoreError> {
            self.session
                .clone()
                .ok_or_else(|| StoreError::NotFound("not found".into()))
        }
        async fn append_message(
            &self,
            _id: SessionId,
            _msg: agent_core::models::Message,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn append_skill_run(
            &self,
            _id: SessionId,
            _run: agent_core::models::SkillRun,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn update_settings(
            &self,
            _id: SessionId,
            _s: SessionSettings,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn archive(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn touch(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn list_archivable(
            &self,
            _before: chrono::DateTime<Utc>,
        ) -> Result<Vec<SessionId>, StoreError> {
            Ok(vec![])
        }
        async fn append_dataset(
            &self,
            _id: SessionId,
            _dataset: agent_core::models::DatasetSummary,
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    // --- Helper ---

    fn build_test_app(state: AppState) -> Router {
        Router::new()
            .route("/api/sessions/:sid/audio", post(post_audio))
            .with_state(state)
    }

    fn make_state_with_stt(sid: Uuid, stt: MockStt) -> AppState {
        let mut state = AppState::new(Arc::new(MockSessionStore::with_active_session(sid)));
        state.stt_provider = Some(Arc::new(stt));
        state
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_post_audio_success_high_confidence() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("帮我做线性回归", 0.95);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 1024]; // 1KB fake audio
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "10")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: PostAudioResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.text, "帮我做线性回归");
        assert!((result.confidence - 0.95).abs() < f32::EPSILON);
        assert!(result.auto_processed);
    }

    #[tokio::test]
    async fn test_post_audio_low_confidence_not_auto_processed() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("模糊内容", 0.4);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 512];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "5")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: PostAudioResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.text, "模糊内容");
        assert!(result.confidence < 0.6);
        assert!(!result.auto_processed);
    }

    #[tokio::test]
    async fn test_post_audio_duration_exceeds_limit() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("不会到达", 0.9);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "61") // exceeds 60s limit
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_audio_size_exceeds_limit() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("不会到达", 0.9);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        // 10MB + 1 byte
        let audio_bytes = vec![0u8; 10 * 1024 * 1024 + 1];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "30")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_audio_missing_duration_header() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("不会到达", 0.9);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            // No X-Audio-Duration-Secs header
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_audio_session_not_found() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("text", 0.9);
        let mut state = AppState::new(Arc::new(MockSessionStore::not_found()));
        state.stt_provider = Some(Arc::new(stt));
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "5")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_post_audio_session_archived() {
        let sid = Uuid::new_v4();
        let stt = MockStt::with_text("text", 0.9);
        let mut state = AppState::new(Arc::new(MockSessionStore::with_archived_session(sid)));
        state.stt_provider = Some(Arc::new(stt));
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "5")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_post_audio_no_stt_provider() {
        let sid = Uuid::new_v4();
        // No STT provider configured
        let state = AppState::new(Arc::new(MockSessionStore::with_active_session(sid)));
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "5")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // Should return 502 (LlmUnavailable maps to 502)
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_post_audio_stt_failure() {
        let sid = Uuid::new_v4();
        let stt = MockStt::new(vec![MockSttResponse::Error(
            agent_core::traits::stt_provider::SttError::TranscriptionFailed {
                reason: "timeout".to_string(),
            },
        )]);
        let state = make_state_with_stt(sid, stt);
        let app = build_test_app(state);

        let audio_bytes = vec![0u8; 100];
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/audio"))
            .header("content-type", "application/octet-stream")
            .header("X-Audio-Duration-Secs", "5")
            .body(Body::from(audio_bytes))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }
}
