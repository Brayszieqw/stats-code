//! Integration tests for agent-server (task 13.1).
//!
//! Covers four end-to-end scenarios using the full HTTP layer with mocked
//! domain services (`MemSessionStore`, `MockMessageHandler`, `MockDatasetStore`).
//!
//! Scenario 1: Create Session → POST message → verify SSE response starts
//! Scenario 2: Upload oversized dataset → `DATASET_TOO_LARGE`
//! Scenario 3: Message handler returns `LlmUnavailable` error
//! Scenario 4: Create → use → archive → write fails with `SESSION_ARCHIVED`
//!
//! Validates: Requirements 3.4, 8.4, 11.3, 11.4

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use base64::Engine;
use tower::ServiceExt;

use agent_core::models::{
    ColumnSummary, ColumnType, DatasetRef, DatasetSummary, Encoding, ErrorPayload, SessionId,
};
use agent_core::orchestrator::{AgentEvent, UserMessageInput};
use agent_core::store::MemSessionStore;
use agent_core::traits::dataset_store::DatasetStore;
use agent_core::traits::session_store::{SessionStore, StoreError};
use agent_server::middleware::load_shedding::LoadCounter;
use agent_server::state::{AppState, MessageHandler};
use bytes::Bytes;
use chrono::Utc;
use tokio_stream::Stream;

// ═══════════════════════════════════════════════════════════════════════════════
// Mock implementations
// ═══════════════════════════════════════════════════════════════════════════════

/// Mock message handler that returns a simple text response + Done event.
struct MockMessageHandlerOk;

impl MessageHandler for MockMessageHandlerOk {
    fn handle_message(
        &self,
        _sid: SessionId,
        _msg: UserMessageInput,
    ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>
    {
        Box::pin(async {
            let events = vec![
                AgentEvent::TextDelta("统计分析开始".to_string()),
                AgentEvent::Interpretation("根据数据分析结果...".to_string()),
                AgentEvent::Done,
            ];
            Box::pin(tokio_stream::iter(events)) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
        })
    }
}

/// Mock message handler that returns an `LLM_UNAVAILABLE` error (simulating `DeepSeek` 502 after retries).
struct MockMessageHandlerLlmUnavailable;

impl MessageHandler for MockMessageHandlerLlmUnavailable {
    fn handle_message(
        &self,
        _sid: SessionId,
        _msg: UserMessageInput,
    ) -> Pin<Box<dyn Future<Output = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>> + Send + '_>>
    {
        Box::pin(async {
            let events = vec![
                AgentEvent::Error(ErrorPayload::new(
                    agent_core::models::ErrorCode::LlmUnavailable,
                    "AI 服务暂时不可用，可稍后重试",
                )),
                AgentEvent::Done,
            ];
            Box::pin(tokio_stream::iter(events)) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
        })
    }
}

/// Mock dataset store that succeeds for normal files.
struct MockDatasetStoreOk;

#[async_trait::async_trait]
impl DatasetStore for MockDatasetStoreOk {
    async fn save_raw(
        &self,
        sid: SessionId,
        name: &str,
        _bytes: Bytes,
    ) -> Result<DatasetRef, StoreError> {
        Ok(DatasetRef {
            session_id: sid,
            dataset_id: uuid::Uuid::new_v4(),
            raw_path: std::path::PathBuf::from(format!("/tmp/{name}")),
        })
    }

    async fn parse(&self, _dref: DatasetRef) -> Result<DatasetSummary, StoreError> {
        Ok(DatasetSummary {
            dataset_id: uuid::Uuid::new_v4(),
            file_name: "test.csv".to_string(),
            size_bytes: 1024,
            encoding: Encoding::Utf8,
            row_count: 10,
            columns: vec![
                ColumnSummary {
                    name: "age".to_string(),
                    inferred_type: ColumnType::Numeric,
                    missing_count: 0,
                },
                ColumnSummary {
                    name: "name".to_string(),
                    inferred_type: ColumnType::String,
                    missing_count: 1,
                },
            ],
            uploaded_at: Utc::now(),
            sha256: None,
        })
    }

    async fn delete_session_data(&self, _sid: SessionId) -> Result<(), StoreError> {
        Ok(())
    }

    async fn read_raw_by_id(&self, _dataset_id: uuid::Uuid) -> Result<Vec<u8>, StoreError> {
        Ok(Vec::new())
    }

    async fn quota_used(&self, _sid: SessionId) -> Result<u64, StoreError> {
        Ok(0)
    }

    fn get_path(&self, _sid: SessionId, _dataset_id: uuid::Uuid, name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(format!("/tmp/{name}"))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═══════════════════════════════════════════════════════════════════════════════

/// Build the full application router with `MemSessionStore` and optional handlers.
fn build_app(
    session_store: Arc<MemSessionStore>,
    message_handler: Option<Arc<dyn MessageHandler>>,
    dataset_store: Option<Arc<dyn DatasetStore>>,
) -> axum::Router {
    let mut app_state = AppState::new(session_store);
    app_state.message_handler = message_handler;
    app_state.dataset_store = dataset_store;
    agent_server::build_router(LoadCounter::new(50), app_state)
}

/// Extract JSON body from a response.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Extract raw body bytes from a response.
async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
    axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

/// Create a session via POST /api/sessions and return its ID string.
async fn create_session(app: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    body["id"].as_str().unwrap().to_string()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 1: Create Session → POST message → receive SSE stream
// ═══════════════════════════════════════════════════════════════════════════════
// Validates: R1.2, R1.3, R9.5

#[tokio::test]
async fn scenario_1_create_session_send_message_receive_sse() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerOk)),
        Some(Arc::new(MockDatasetStoreOk)),
    );

    // Step 1: Create session
    let sid = create_session(&app).await;

    // Step 2: Send a text message
    let msg_body = serde_json::json!({ "text": "帮我做线性回归分析" });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/messages"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&msg_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();

    // Step 3: Verify SSE response
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        ct.contains("text/event-stream"),
        "Expected SSE content type, got: {ct}"
    );

    // Step 4: Verify X-Request-Id is present
    let request_id = resp.headers().get("x-request-id");
    assert!(request_id.is_some(), "Response must have X-Request-Id");

    // Step 5: Read SSE body and verify it contains expected events
    let raw_body = body_bytes(resp).await;
    let body_str = String::from_utf8_lossy(&raw_body);
    assert!(
        body_str.contains("event: text_delta"),
        "SSE stream should contain text_delta event"
    );
    assert!(
        body_str.contains("event: interpretation"),
        "SSE stream should contain interpretation event"
    );
    assert!(
        body_str.contains("event: done"),
        "SSE stream should contain done event"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 2: Upload oversized dataset → DATASET_TOO_LARGE
// ═══════════════════════════════════════════════════════════════════════════════
// Validates: R3.4

#[tokio::test]
async fn scenario_2_upload_oversized_file_returns_dataset_too_large() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerOk)),
        Some(Arc::new(MockDatasetStoreOk)),
    );

    // Step 1: Create session
    let sid = create_session(&app).await;

    // Step 2: Create a base64-encoded payload that exceeds 50 MB when decoded.
    // We send a JSON body with base64 data. The decoded size must exceed 50MB.
    // Use exactly 50MB + 1 byte to be just over the limit.
    let oversized_data = vec![0u8; 50 * 1024 * 1024 + 1]; // 50 MB + 1 byte
    let encoded = base64::engine::general_purpose::STANDARD.encode(&oversized_data);

    // Build the JSON body manually as a string to avoid serde_json parsing overhead
    let json_body = format!(
        r#"{{"filename":"large_data.csv","data":"{encoded}"}}"#
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/datasets"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();

    // Step 3: Verify DATASET_TOO_LARGE (HTTP 413)
    // Note: the response may come from axum's body limit (non-JSON) or our handler (JSON).
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "Oversized file should return 413"
    );

    // Try to parse as JSON; if axum's body limit rejects first, body may not be JSON
    let raw_bytes = body_bytes(resp).await;
    if let Ok(resp_body) = serde_json::from_slice::<serde_json::Value>(&raw_bytes) {
        assert_eq!(resp_body["error_code"], "DatasetTooLarge");
        assert!(
            resp_body["message"].as_str().unwrap().contains("过大"),
            "Error message should mention '过大': {}",
            resp_body["message"]
        );
    }
    // If body is not JSON, the 413 status alone validates the requirement.
}

#[tokio::test]
async fn dataset_upload_over_axum_default_limit_reaches_handler() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerOk)),
        Some(Arc::new(MockDatasetStoreOk)),
    );

    let sid = create_session(&app).await;
    let data = vec![b'a'; 3 * 1024 * 1024];
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let json_body = format!(
        r#"{{"filename":"medium.csv","data":"{encoded}"}}"#
    );

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/datasets"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn audio_upload_over_axum_default_limit_reaches_handler() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerOk)),
        Some(Arc::new(MockDatasetStoreOk)),
    );

    let sid = create_session(&app).await;
    let audio = vec![0u8; 3 * 1024 * 1024];

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/audio"))
        .header("X-Audio-Duration-Secs", "1")
        .body(Body::from(audio))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    let body = body_json(resp).await;
    assert_eq!(body["error_code"], "LlmUnavailable");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 3: DeepSeek 502 → retries exhausted → LLM_UNAVAILABLE
// ═══════════════════════════════════════════════════════════════════════════════
// Validates: R8.4

#[tokio::test]
async fn scenario_3_llm_unavailable_returns_error_in_sse_stream() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerLlmUnavailable)),
        None,
    );

    // Step 1: Create session
    let sid = create_session(&app).await;

    // Step 2: Send a message that triggers LLM failure
    let msg_body = serde_json::json!({ "text": "分析数据" });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/messages"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&msg_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();

    // Step 3: Verify we get an SSE stream (the error is delivered via SSE)
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"));

    // Step 4: Read the SSE body and verify it contains an error event with LlmUnavailable
    let raw_body = body_bytes(resp).await;
    let body_str = String::from_utf8_lossy(&raw_body);
    assert!(
        body_str.contains("event: error"),
        "SSE stream should contain an error event"
    );
    assert!(
        body_str.contains("LlmUnavailable"),
        "SSE error event should contain LlmUnavailable error code"
    );
    assert!(
        body_str.contains("event: done"),
        "SSE stream should still terminate with done event"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Scenario 4: 24h inactive → archive → write returns SESSION_ARCHIVED
// ═══════════════════════════════════════════════════════════════════════════════
// Validates: R11.3, R11.4

#[tokio::test]
async fn scenario_4_archived_session_rejects_writes_allows_reads() {
    let store = Arc::new(MemSessionStore::new());
    let app = build_app(
        store.clone(),
        Some(Arc::new(MockMessageHandlerOk)),
        Some(Arc::new(MockDatasetStoreOk)),
    );

    // Step 1: Create a session
    let sid = create_session(&app).await;

    // Step 2: Verify the session is initially Active
    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/sessions/{sid}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = body_json(get_resp).await;
    assert_eq!(body["status"], "Active");

    // Step 3: Archive the session (simulating 24h inactivity via direct store access)
    let session_id = SessionId(uuid::Uuid::parse_str(&sid).unwrap());
    store.archive(session_id).await.unwrap();

    // Step 4: Verify read (GET) still works on archived session
    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/sessions/{sid}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = body_json(get_resp).await;
    assert_eq!(body["status"], "Archived");

    // Step 5: Verify write (PATCH settings) returns 409 SESSION_ARCHIVED
    let patch_body = serde_json::json!({ "decision_assistant": false });
    let patch_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/sessions/{sid}/settings"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body).unwrap()))
        .unwrap();
    let patch_resp = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(patch_resp.status(), StatusCode::CONFLICT);
    let err_body = body_json(patch_resp).await;
    assert_eq!(err_body["error_code"], "SessionArchived");
    assert!(err_body["message"].as_str().unwrap().contains("归档"));

    // Step 6: Verify write (POST message) also returns 409 SESSION_ARCHIVED
    let msg_body = serde_json::json!({ "text": "hello" });
    let msg_req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/messages"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&msg_body).unwrap()))
        .unwrap();
    let msg_resp = app.clone().oneshot(msg_req).await.unwrap();
    assert_eq!(msg_resp.status(), StatusCode::CONFLICT);
    let err_body = body_json(msg_resp).await;
    assert_eq!(err_body["error_code"], "SessionArchived");

    // Step 7: Verify write (POST dataset) also returns 409 SESSION_ARCHIVED
    let dataset_body = serde_json::json!({
        "filename": "data.csv",
        "data": base64::engine::general_purpose::STANDARD.encode(b"col1\n1")
    });
    let ds_req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{sid}/datasets"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&dataset_body).unwrap()))
        .unwrap();
    let ds_resp = app.clone().oneshot(ds_req).await.unwrap();
    assert_eq!(ds_resp.status(), StatusCode::CONFLICT);
    let err_body = body_json(ds_resp).await;
    assert_eq!(err_body["error_code"], "SessionArchived");
}
