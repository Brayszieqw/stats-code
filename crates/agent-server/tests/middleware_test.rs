//! Integration tests for `request_id` and `load_shedding` middleware.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt; // for `oneshot`

// Import the public build_router function and LoadCounter
use agent_core::store::MemSessionStore;
use agent_server::middleware::load_shedding::LoadCounter;
use agent_server::state::AppState;

/// Helper: build a test router with the given concurrency threshold.
fn test_app(threshold: u32) -> axum::Router {
    let session_store = Arc::new(MemSessionStore::new());
    let app_state = AppState::new(session_store);
    agent_server::build_router(LoadCounter::new(threshold), app_state)
}

#[tokio::test]
async fn health_returns_ok() {
    let app = test_app(50);
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn response_contains_x_request_id_header() {
    let app = test_app(50);
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let header = resp.headers().get("x-request-id");
    assert!(header.is_some(), "response must have X-Request-Id header");

    let value = header.unwrap().to_str().unwrap();
    assert!(!value.is_empty(), "X-Request-Id must be non-empty");

    // Should be a valid UUID v4
    let parsed = uuid::Uuid::parse_str(value);
    assert!(parsed.is_ok(), "X-Request-Id must be a valid UUID, got: {value}");
}

#[tokio::test]
async fn no_x_server_load_header_under_threshold() {
    let app = test_app(50);
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Under threshold: no X-Server-Load header (or "normal")
    let header = resp.headers().get("x-server-load");
    match header {
        None => {} // OK: no header when under threshold
        Some(v) => {
            let val = v.to_str().unwrap();
            assert_eq!(val, "normal", "if present, should be 'normal' under threshold");
        }
    }
}

#[tokio::test]
async fn x_server_load_degraded_above_threshold() {
    // Use threshold=0 so even a single request triggers degraded.
    let app = test_app(0);
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let header = resp
        .headers()
        .get("x-server-load")
        .expect("X-Server-Load header must be present when above threshold");
    assert_eq!(header.to_str().unwrap(), "degraded");
}

#[tokio::test]
async fn never_returns_503_regardless_of_load() {
    // Even with threshold=0 (every request is "degraded"), status must NOT be 503.
    let app = test_app(0);
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn each_request_gets_unique_request_id() {
    let app = test_app(50);

    let req1 = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    let id1 = resp1
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let req2 = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    let id2 = resp2
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    assert_ne!(id1, id2, "each request should get a unique request ID");
}
