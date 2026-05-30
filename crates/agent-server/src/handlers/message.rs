//! Message-related HTTP handlers.
//!
//! - `POST /api/sessions/:sid/messages` → [`post_message`]
//!
//! Validates message length, checks session existence and status,
//! then calls the orchestrator and streams `AgentEvent`s back via SSE.

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::StreamExt;
use serde::Deserialize;
use tokio_stream::Stream;
use uuid::Uuid;

use agent_core::models::{ErrorCode, ErrorPayload, SessionStatus};
use agent_core::orchestrator::{AgentEvent, UserMessageInput};
use agent_core::validation::message::validate_message_length;

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request body
// ---------------------------------------------------------------------------

/// Request body for `POST /api/sessions/:sid/messages`.
///
/// Accepts either `{ "text": "..." }` or `{ "content": { "type": "text", "text": "..." } }`.
#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    /// Direct text field (preferred simple format).
    pub text: Option<String>,
    /// Structured content field (alternative format).
    pub content: Option<ContentPayload>,
}

/// Structured content payload for messages.
#[derive(Debug, Deserialize)]
pub struct ContentPayload {
    /// Content type discriminator (currently only "text" is supported).
    #[serde(rename = "type")]
    pub content_type: String,
    /// The text content.
    pub text: String,
}

impl PostMessageRequest {
    /// Extract the message text from either format.
    ///
    /// Returns `None` if neither `text` nor a valid `content.text` is provided.
    fn extract_text(&self) -> Option<&str> {
        if let Some(ref t) = self.text {
            return Some(t.as_str());
        }
        if let Some(ref c) = self.content {
            if c.content_type == "text" {
                return Some(c.text.as_str());
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/sessions/:sid/messages` — send a user message and receive an SSE event stream.
///
/// Flow:
/// 1. Parse JSON body and extract text
/// 2. Validate message length (≤ 8000 chars)
/// 3. Check session exists and is not archived
/// 4. Call `MessageHandler::handle_message` to get a stream of `AgentEvent`
/// 5. Map each `AgentEvent` to an SSE `Event` and stream to the client
///
/// Each `AgentEvent` variant maps to a distinct SSE `event:` field:
/// - `TextDelta` → `event: text_delta`
/// - `ChoicePrompt` → `event: choice_prompt`
/// - `SkillCall` → `event: skill_call`
/// - `SkillResult` → `event: skill_result`
/// - `Interpretation` → `event: interpretation`
/// - `Error` → `event: error`
/// - `Done` → `event: done`
pub async fn post_message(
    State(state): State<AppState>,
    Path(sid): Path<Uuid>,
    Json(body): Json<PostMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    // 1. Extract text from request body
    let text = body.extract_text().ok_or_else(|| {
        AppError(ErrorPayload::new(
            ErrorCode::MessageTooLong, // reuse closest code; could be a separate InvalidInput
            "请求体缺少 text 字段",
        ))
    })?;

    // 2. Validate message length
    validate_message_length(text).map_err(AppError)?;

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

    // 4. Get the message handler (orchestrator)
    let handler = state.message_handler.as_ref().ok_or_else(|| {
        AppError(ErrorPayload::new(
            ErrorCode::LlmUnavailable,
            "AI 服务尚未初始化",
        ))
    })?;

    let msg_input = UserMessageInput {
        text: text.to_owned(),
        settings: session.settings.clone(),
    };

    // 5. Call orchestrator to get the event stream
    let event_stream = handler.handle_message(session_id, msg_input).await;

    // 6. Map AgentEvent to SSE Event
    let sse_stream = event_stream.map(|agent_event| {
        Ok::<_, std::convert::Infallible>(agent_event_to_sse(agent_event))
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// SSE event mapping
// ---------------------------------------------------------------------------

/// Convert an `AgentEvent` into an axum SSE `Event`.
///
/// Each variant gets a distinct `event:` type and JSON-serialized `data:`.
fn agent_event_to_sse(event: AgentEvent) -> Event {
    match event {
        AgentEvent::TextDelta(text) => Event::default()
            .event("text_delta")
            .data(serde_json::json!({ "text": text }).to_string()),

        AgentEvent::AnalysisPlan(plan) => Event::default()
            .event("analysis_plan")
            .data(serde_json::to_string(&plan).unwrap_or_default()),

        AgentEvent::ChoicePrompt(prompt) => Event::default()
            .event("choice_prompt")
            .data(serde_json::to_string(&prompt).unwrap_or_default()),

        AgentEvent::SkillCall { skill_id, args } => Event::default()
            .event("skill_call")
            .data(serde_json::json!({ "skill_id": skill_id, "args": args }).to_string()),

        AgentEvent::SkillResult(result) => Event::default()
            .event("skill_result")
            .data(serde_json::to_string(&result).unwrap_or_default()),

        AgentEvent::Interpretation(text) => Event::default()
            .event("interpretation")
            .data(serde_json::json!({ "text": text }).to_string()),

        AgentEvent::Error(payload) => Event::default()
            .event("error")
            .data(serde_json::to_string(&payload).unwrap_or_default()),

        AgentEvent::Done => Event::default().event("done").data("{}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MessageHandler;
    use agent_core::models::{
        AnalysisPlan, AnalysisPlanStep, AnalysisStepStatus, AnalysisTaskType, Session, SessionId,
        SessionSettings, SessionStatus,
    };
    use agent_core::orchestrator::AgentEvent;
    use agent_core::traits::session_store::{SessionStore, StoreError};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use chrono::Utc;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio_stream::Stream;
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

    // --- Mock MessageHandler ---

    struct MockMessageHandler;

    impl MessageHandler for MockMessageHandler {
        fn handle_message(
            &self,
            _sid: SessionId,
            _msg: UserMessageInput,
        ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>
        {
            Box::pin(async {
                let events = vec![
                    AgentEvent::TextDelta("你好".to_string()),
                    AgentEvent::Done,
                ];
                Box::pin(tokio_stream::iter(events)) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
            })
        }
    }

    struct MockMessageHandlerPlan;

    impl MessageHandler for MockMessageHandlerPlan {
        fn handle_message(
            &self,
            _sid: SessionId,
            _msg: UserMessageInput,
        ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>
        {
            Box::pin(async {
                let plan = AnalysisPlan {
                    plan_id: Uuid::new_v4(),
                    task_type: AnalysisTaskType::Regression,
                    target_skill_id: Some("model_linear".to_string()),
                    requires_user_input: false,
                    steps: vec![AnalysisPlanStep {
                        order: 1,
                        title: "Classify request".to_string(),
                        detail: "Task type: Regression".to_string(),
                        skill_id: None,
                        status: AnalysisStepStatus::Planned,
                    }],
                };
                let events = vec![AgentEvent::AnalysisPlan(plan), AgentEvent::Done];
                Box::pin(tokio_stream::iter(events)) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
            })
        }
    }

    // --- Helper ---

    fn build_test_app(state: AppState) -> Router {
        Router::new()
            .route("/api/sessions/:sid/messages", post(post_message))
            .with_state(state)
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_post_message_success_returns_sse() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_active_session(sid)),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({ "text": "帮我做线性回归" });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // SSE responses have text/event-stream content type
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/event-stream"));
    }

    #[tokio::test]
    async fn test_post_message_content_format() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_active_session(sid)),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({
            "content": { "type": "text", "text": "帮我做回归" }
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_post_message_streams_analysis_plan_event() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_active_session(sid)),
            Arc::new(MockMessageHandlerPlan),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({ "text": "run regression" });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let raw_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&raw_body);
        assert!(body_str.contains("event: analysis_plan"));
        assert!(body_str.contains("\"task_type\":\"Regression\""));
        assert!(body_str.contains("event: done"));
    }

    #[tokio::test]
    async fn test_post_message_too_long() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_active_session(sid)),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let long_text: String = std::iter::repeat('字').take(8001).collect();
        let body = serde_json::json!({ "text": long_text });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_message_session_not_found() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::not_found()),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({ "text": "hello" });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_post_message_session_archived() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_archived_session(sid)),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({ "text": "hello" });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_post_message_missing_text() {
        let sid = Uuid::new_v4();
        let state = AppState::with_message_handler(
            Arc::new(MockSessionStore::with_active_session(sid)),
            Arc::new(MockMessageHandler),
        );
        let app = build_test_app(state);

        let body = serde_json::json!({});
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // Missing text field → 413 (using MessageTooLong code)
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
