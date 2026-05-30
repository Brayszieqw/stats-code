//! `POST /api/sidecar/{algorithm_id}` —
//! returns the Equivalent Code Sidecar snippet DTO for one
//! `(algorithm_id, software)` cell (task 10.3).
//!
//! - The handler delegates to [`crate::state::SidecarProvider`]; the
//!   launcher in `stats-code` injects a concrete provider that calls
//!   `stats_code::sidecar::generate_snippet`.
//! - The Equivalent Code Sidecar is a pure function of
//!   `(algorithm_id, software, columns, dataset_sha256, params)`, so the
//!   SPA posts those fields directly in a [`SidecarRenderRequest`] body —
//!   no server-side run state is consulted. This is what lets the endpoint
//!   render real snippets end-to-end.
//! - 200 carries a [`SidecarSnippetDto`] for **all four** coverage
//!   states (Requirement 1.5/1.6/6.4): `none` returns the DTO with
//!   `coverage_value = "none"` and no `text`, letting the SPA render
//!   the placeholder client-side without a special status code.
//! - 4xx is reserved for caller-side errors (unknown algorithm, invalid
//!   request, redaction violation, forbidden spawn) — see the match
//!   arms below.
//!
//! Validates: Requirements 1.3 (transport), 1.5 (uncovered DTO),
//! 2.2 (snippet generation contract), 6.2 (matrix-driven dispatch),
//! 10.3 (same-process axum handler).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use api::sidecar::SidecarRenderRequest;

use crate::state::{AppState, SidecarProviderError};

/// `POST /api/sidecar/{algorithm_id}` handler.
pub async fn post_sidecar(
    State(state): State<AppState>,
    Path(algorithm_id): Path<String>,
    Json(request): Json<SidecarRenderRequest>,
) -> Response {
    let Some(provider) = state.sidecar_provider.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error_code": "SidecarUnavailable",
                "message": "sidecar provider not configured",
            })),
        )
            .into_response();
    };

    match provider.generate(&algorithm_id, &request) {
        Ok(dto) => (StatusCode::OK, Json(dto)).into_response(),
        Err(SidecarProviderError::UnknownAlgorithm(id)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error_code": "UnknownAlgorithm",
                "message": format!("unknown algorithm: {id}"),
                "algorithm_id": id,
            })),
        )
            .into_response(),
        Err(SidecarProviderError::MissingTemplate {
            algorithm_id: id,
            software,
        }) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error_code": "MissingTemplate",
                "message": "no sidecar template for algorithm/software pair",
                "algorithm_id": id,
                "software": software,
            })),
        )
            .into_response(),
        Err(SidecarProviderError::InvalidRequest(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error_code": "InvalidRequest",
                "message": msg,
            })),
        )
            .into_response(),
        Err(SidecarProviderError::RedactionViolation(msg)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error_code": "RedactionViolation",
                "message": msg,
            })),
        )
            .into_response(),
        Err(SidecarProviderError::ForbiddenSpawn(msg)) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error_code": "ForbiddenSpawn",
                "message": msg,
            })),
        )
            .into_response(),
        Err(SidecarProviderError::Internal(msg)) => (
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
    use api::sidecar::{
        CoverageValueDto, ReferenceSoftware, SidecarRenderRequest, SidecarSnippetDto,
    };
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    use crate::state::SidecarProvider;

    struct FixedProvider(Result<SidecarSnippetDto, SidecarProviderError>);

    impl SidecarProvider for FixedProvider {
        fn generate(
            &self,
            _algorithm_id: &str,
            _request: &SidecarRenderRequest,
        ) -> Result<SidecarSnippetDto, SidecarProviderError> {
            self.0.clone()
        }
    }

    fn build_app(state: AppState) -> Router {
        Router::new()
            .route("/api/sidecar/:algorithm_id", post(post_sidecar))
            .with_state(state)
    }

    fn request_body(software: &str) -> Body {
        let body = serde_json::json!({
            "software": software,
            "dataset_sha256": "a".repeat(64),
            "columns": [{ "name": "age", "dtype": "numeric" }],
            "params": {},
        });
        Body::from(serde_json::to_vec(&body).unwrap())
    }

    fn post_request(algorithm_id: &str, software: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!("/api/sidecar/{algorithm_id}"))
            .header("content-type", "application/json")
            .body(request_body(software))
            .unwrap()
    }

    fn live_snippet() -> SidecarSnippetDto {
        SidecarSnippetDto {
            algorithm_id: "tableone".into(),
            software: ReferenceSoftware::R,
            coverage_value: CoverageValueDto::Live,
            text: Some("# header\nlibrary(tableone)\n".into()),
            sha256_of_dataset: "a".repeat(64),
            release_version: "0.5.0".into(),
        }
    }

    fn uncovered_snippet() -> SidecarSnippetDto {
        SidecarSnippetDto {
            algorithm_id: "tableone".into(),
            software: ReferenceSoftware::SPSS,
            coverage_value: CoverageValueDto::None_,
            text: None,
            sha256_of_dataset: "0".repeat(64),
            release_version: "0.5.0".into(),
        }
    }

    #[tokio::test]
    async fn returns_200_with_snippet_for_live_cell() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Ok(live_snippet()))));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("tableone", "R"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let dto: SidecarSnippetDto = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(dto, live_snippet());
    }

    #[tokio::test]
    async fn returns_200_with_uncovered_dto_for_none_cell() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Ok(uncovered_snippet()))));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("tableone", "SPSS"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["coverage_value"], "none");
        assert!(v.get("text").is_none(), "text must be absent: {v}");
    }

    #[tokio::test]
    async fn returns_404_for_unknown_algorithm() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Err(
            SidecarProviderError::UnknownAlgorithm("does-not-exist".into()),
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("does-not-exist", "R"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "UnknownAlgorithm");
        assert_eq!(v["algorithm_id"], "does-not-exist");
        assert!(
            v["message"].as_str().unwrap().contains("does-not-exist"),
            "message should reference the unknown id"
        );
    }

    #[tokio::test]
    async fn returns_400_for_invalid_request() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Err(
            SidecarProviderError::InvalidRequest("unknown column dtype: blob".into()),
        ))));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("tableone", "R"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "InvalidRequest");
        assert!(v["message"].as_str().unwrap().contains("dtype"));
    }

    #[tokio::test]
    async fn returns_400_for_invalid_software_token_in_body() {
        // serde rejects an unknown `software` enum tag in the JSON body
        // with a 4xx before the handler runs.
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Ok(live_snippet()))));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("tableone", "Octave"))
            .await
            .unwrap();
        assert!(
            resp.status().is_client_error(),
            "unknown software tag must be a 4xx, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn returns_503_when_provider_absent() {
        let state = AppState::new(Arc::new(MemSessionStore::new()));
        let app = build_app(state);

        let resp = app
            .oneshot(post_request("tableone", "R"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
