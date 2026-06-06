//! `POST /api/snapshot/export` — produces an Audit Snapshot `.zip`
//! for an analysis run (task 10.4).
//!
//! The handler is a thin shell over [`crate::state::SnapshotProvider`].
//! All the heavy lifting (gating on `run.status`, payload measurement,
//! deterministic zip writing, fsync + atomic rename) lives in the
//! `stats-code` crate; the agent-server crate only owns the HTTP
//! status-code mapping for the structured error variants.
//!
//! Status codes:
//!
//! - 200 — `SnapshotExportResponse { snapshot_path, sha256 }`.
//! - 404 — run does not exist.
//! - 409 — run status is not `completed` (Requirement 7.8). Body
//!   carries the actual status as required.
//! - 413 — measured artifact payload exceeds the 50 MB ceiling
//!   (Requirement 7.7). Body carries `measured_bytes` and
//!   `ceiling_bytes`.
//! - 403 — runtime sentinel detected a forbidden spawn or library load
//!   (Requirement 10.5).
//! - 500 — internal exporter error.
//! - 503 — exporter not configured (early-boot guard).
//!
//! Validates: Requirements 7.1, 7.6, 7.7, 7.8, 10.3, 10.5.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use api::sidecar::SnapshotExportRequest;

use crate::state::{AppState, SnapshotProviderError};

/// `POST /api/snapshot/export` handler.
pub async fn post_snapshot_export(
    State(state): State<AppState>,
    Json(req): Json<SnapshotExportRequest>,
) -> Response {
    let Some(provider) = state.snapshot_provider.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error_code": "SnapshotUnavailable",
                "message": "snapshot provider not configured",
            })),
        )
            .into_response();
    };

    match provider.export(&req.run_id, &req.destination) {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(SnapshotProviderError::UnknownRun(run_id)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error_code": "UnknownRun",
                "message": format!("unknown run id: {run_id}"),
                "run_id": run_id,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::RunNotCompleted { actual_status }) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error_code": "RunNotCompleted",
                "message": format!(
                    "run status is {actual_status}; snapshot export requires completed",
                ),
                "actual_status": actual_status,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::NoExportableStep { run_id }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error_code": "no_exportable_step",
                "message": format!(
                    "run {run_id} has no completed workflow step that can be exported",
                ),
                "run_id": run_id,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::DatasetUnresolved { reason }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error_code": "dataset_unresolved",
                "message": format!(
                    "cannot resolve dataset for export: {reason}",
                ),
                "reason": reason,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::PayloadTooLarge {
            measured_bytes,
            ceiling_bytes,
        }) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error_code": "PayloadTooLarge",
                "message": format!(
                    "artifact payload {measured_bytes} bytes exceeds {ceiling_bytes} byte ceiling",
                ),
                "measured_bytes": measured_bytes,
                "ceiling_bytes": ceiling_bytes,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::ForbiddenSpawn(msg)) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error_code": "ForbiddenSpawn",
                "message": msg,
            })),
        )
            .into_response(),
        Err(SnapshotProviderError::Internal(msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error_code": "InternalError",
                "message": msg,
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use agent_core::store::MemSessionStore;
    use api::sidecar::SnapshotExportResponse;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use crate::state::SnapshotProvider;

    struct FixedProvider(Result<SnapshotExportResponse, SnapshotProviderError>);

    impl SnapshotProvider for FixedProvider {
        fn export(
            &self,
            _run_id: &str,
            _destination: &str,
        ) -> Result<SnapshotExportResponse, SnapshotProviderError> {
            self.0.clone()
        }
    }

    fn build_app(state: AppState) -> Router {
        Router::new()
            .route("/api/snapshot/export", post(post_snapshot_export))
            .with_state(state)
    }

    fn make_request(run_id: &str, destination: &str) -> Request<Body> {
        let body = serde_json::to_vec(&serde_json::json!({
            "run_id": run_id,
            "destination": destination,
        }))
        .unwrap();
        Request::builder()
            .method("POST")
            .uri("/api/snapshot/export")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn returns_200_with_response_on_success() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Ok(SnapshotExportResponse {
            snapshot_path: "C:/tmp/out.zip".into(),
            sha256: "f".repeat(64),
        }))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-1", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: SnapshotExportResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body.snapshot_path, "C:/tmp/out.zip");
        assert_eq!(body.sha256.len(), 64);
    }

    #[tokio::test]
    async fn returns_409_with_actual_status_when_run_not_completed() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Err(
            SnapshotProviderError::RunNotCompleted {
                actual_status: "running".into(),
            },
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-2", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "RunNotCompleted");
        assert_eq!(v["actual_status"], "running");
        assert!(
            v["message"].as_str().unwrap().contains("running"),
            "message should contain the actual status"
        );
    }

    #[tokio::test]
    async fn returns_413_with_measurement_when_payload_too_large() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Err(
            SnapshotProviderError::PayloadTooLarge {
                measured_bytes: 60 * 1024 * 1024,
                ceiling_bytes: 50 * 1024 * 1024,
            },
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-3", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "PayloadTooLarge");
        assert_eq!(v["measured_bytes"], 60 * 1024 * 1024);
        assert_eq!(v["ceiling_bytes"], 50 * 1024 * 1024);
        assert!(
            v["message"].as_str().unwrap().contains("exceeds"),
            "message should describe the ceiling violation"
        );
    }

    #[tokio::test]
    async fn returns_404_when_run_unknown() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Err(
            SnapshotProviderError::UnknownRun("missing".into()),
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("missing", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_503_when_provider_absent() {
        let state = AppState::new(Arc::new(MemSessionStore::new()));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-4", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn returns_422_when_no_exportable_step() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Err(
            SnapshotProviderError::NoExportableStep {
                run_id: "run-5".into(),
            },
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-5", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "no_exportable_step");
        assert_eq!(v["run_id"], "run-5");
        assert!(
            v["message"]
                .as_str()
                .unwrap()
                .contains("no completed workflow step"),
            "message should describe the refusal reason"
        );
    }

    #[tokio::test]
    async fn returns_422_when_dataset_unresolved() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.snapshot_provider = Some(Arc::new(FixedProvider(Err(
            SnapshotProviderError::DatasetUnresolved {
                reason: "dataset file not found on disk".into(),
            },
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(make_request("run-6", "C:/tmp/out.zip"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "dataset_unresolved");
        assert_eq!(v["reason"], "dataset file not found on disk");
        assert!(
            v["message"]
                .as_str()
                .unwrap()
                .contains("cannot resolve dataset"),
            "message should describe the refusal reason"
        );
    }
}
