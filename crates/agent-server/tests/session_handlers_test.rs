//! Integration tests for session HTTP handlers (task 9.3).
//!
//! Validates:
//! - POST /api/sessions → 201 with new session
//! - GET /api/sessions/:sid → 200 with session, 404 for non-existent
//! - PATCH /api/sessions/:sid/settings → 200 with updated session, 404/409
//! - Archived session write → 409 SESSION_ARCHIVED

use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use tower::ServiceExt;

use agent_core::store::MemSessionStore;
use agent_server::middleware::load_shedding::LoadCounter;
use agent_server::state::AppState;

/// Helper: build a test router with an in-memory session store.
fn test_app() -> axum::Router {
    let session_store = Arc::new(MemSessionStore::new());
    let app_state = AppState::new(session_store);
    agent_server::build_router(LoadCounter::new(50), app_state)
}

/// Helper: extract JSON body from a response.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// ─── POST /api/sessions ───────────────────────────────────────────────────────

#[tokio::test]
async fn create_session_returns_201() {
    let app = test_app();
    let req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = body_json(resp).await;
    assert!(body["id"].is_string(), "response must have session id");
    assert_eq!(body["status"], "Active");
    assert_eq!(body["settings"]["decision_assistant"], true);
}

// ─── GET /api/sessions/:sid ───────────────────────────────────────────────────

#[tokio::test]
async fn get_session_returns_200_for_existing() {
    let app = test_app();

    // First create a session
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp).await;
    let sid = created["id"].as_str().unwrap();

    // Now get it
    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/sessions/{sid}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);

    let body = body_json(get_resp).await;
    assert_eq!(body["id"], sid);
    assert_eq!(body["status"], "Active");
}

#[tokio::test]
async fn get_session_returns_404_for_nonexistent() {
    let app = test_app();
    let fake_id = uuid::Uuid::new_v4();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/sessions/{fake_id}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = body_json(resp).await;
    assert_eq!(body["error_code"], "SessionNotFound");
    assert!(body["message"].as_str().unwrap().contains("会话"));
}

// ─── PATCH /api/sessions/:sid/settings ────────────────────────────────────────

#[tokio::test]
async fn patch_settings_returns_200_with_updated_session() {
    let app = test_app();

    // Create a session
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let created = body_json(create_resp).await;
    let sid = created["id"].as_str().unwrap();

    // Patch settings: disable decision_assistant
    let patch_body = serde_json::json!({ "decision_assistant": false });
    let patch_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/sessions/{sid}/settings"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body).unwrap()))
        .unwrap();

    let patch_resp = app.oneshot(patch_req).await.unwrap();
    assert_eq!(patch_resp.status(), StatusCode::OK);

    let body = body_json(patch_resp).await;
    assert_eq!(body["settings"]["decision_assistant"], false);
}

#[tokio::test]
async fn patch_settings_returns_404_for_nonexistent() {
    let app = test_app();
    let fake_id = uuid::Uuid::new_v4();

    let patch_body = serde_json::json!({ "decision_assistant": false });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/sessions/{fake_id}/settings"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = body_json(resp).await;
    assert_eq!(body["error_code"], "SessionNotFound");
}

#[tokio::test]
async fn patch_settings_returns_409_for_archived_session() {
    let session_store = Arc::new(MemSessionStore::new());
    let app_state = AppState::new(session_store.clone());
    let app = agent_server::build_router(LoadCounter::new(50), app_state);

    // Create a session
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let created = body_json(create_resp).await;
    let sid_str = created["id"].as_str().unwrap();
    let sid = agent_core::models::SessionId(uuid::Uuid::parse_str(sid_str).unwrap());

    // Archive it directly via the store
    use agent_core::traits::session_store::SessionStore;
    session_store.archive(sid).await.unwrap();

    // Now try to patch settings — should get 409
    let patch_body = serde_json::json!({ "decision_assistant": false });
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/sessions/{sid_str}/settings"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let body = body_json(resp).await;
    assert_eq!(body["error_code"], "SessionArchived");
    assert!(body["message"].as_str().unwrap().contains("归档"));
}

// ─── GET on archived session still works (read-only OK) ───────────────────────

#[tokio::test]
async fn get_session_returns_200_for_archived_session() {
    let session_store = Arc::new(MemSessionStore::new());
    let app_state = AppState::new(session_store.clone());
    let app = agent_server::build_router(LoadCounter::new(50), app_state);

    // Create and archive
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let created = body_json(create_resp).await;
    let sid_str = created["id"].as_str().unwrap();
    let sid = agent_core::models::SessionId(uuid::Uuid::parse_str(sid_str).unwrap());

    use agent_core::traits::session_store::SessionStore;
    session_store.archive(sid).await.unwrap();

    // GET still works
    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/sessions/{sid_str}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);

    let body = body_json(get_resp).await;
    assert_eq!(body["status"], "Archived");
}
