//! `GET /api/sidecar/{algorithm_id}?software=...&run_id=...` —
//! returns the Equivalent Code Sidecar snippet DTO for one
//! `(algorithm_id, software)` cell of the active analysis run
//! (task 10.3).
//!
//! - The handler delegates to [`crate::state::SidecarProvider`]; the
//!   launcher in `stats-code` injects a concrete provider that calls
//!   `stats_code::sidecar::generate_snippet`.
//! - 200 carries a [`SidecarSnippetDto`] for **all four** coverage
//!   states (Requirement 1.5/1.6/6.4): `none` returns the DTO with
//!   `coverage_value = "none"` and no `text`, letting the SPA render
//!   the placeholder client-side without a special status code.
//! - 4xx is reserved for caller-side errors (unknown algorithm, unknown
//!   software, redaction violation, forbidden spawn) — see the match
//!   arms below.
//!
//! Validates: Requirements 1.3 (transport), 1.5 (uncovered DTO),
//! 2.2 (snippet generation contract), 6.2 (matrix-driven dispatch),
//! 10.3 (same-process axum handler).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use api::sidecar::ReferenceSoftware;

use crate::state::{AppState, SidecarProviderError};

/// Query string parameters for `GET /api/sidecar/{algorithm_id}`.
#[derive(Debug, Deserialize)]
pub struct SidecarQuery {
    /// Reference software for the requested tab. Serialized form is
    /// exactly `R | SAS | Python | SPSS`.
    pub software: ReferenceSoftware,
    /// Identifier of the analysis run whose column metadata and
    /// dataset SHA256 are baked into the snippet header.
    pub run_id: String,
}

/// `GET /api/sidecar/{algorithm_id}` handler.
pub async fn get_sidecar(
    State(state): State<AppState>,
    Path(algorithm_id): Path<String>,
    Query(query): Query<SidecarQuery>,
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

    match provider.generate(&algorithm_id, query.software, &query.run_id) {
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
    use api::sidecar::{CoverageValueDto, ReferenceSoftware, SidecarSnippetDto};
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use crate::state::SidecarProvider;

    struct FixedProvider(Result<SidecarSnippetDto, SidecarProviderError>);

    impl SidecarProvider for FixedProvider {
        fn generate(
            &self,
            _algorithm_id: &str,
            _software: ReferenceSoftware,
            _run_id: &str,
        ) -> Result<SidecarSnippetDto, SidecarProviderError> {
            self.0.clone()
        }
    }

    fn build_app(state: AppState) -> Router {
        Router::new()
            .route("/api/sidecar/:algorithm_id", get(get_sidecar))
            .with_state(state)
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
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/sidecar/tableone?software=R&run_id=run-1")
                    .body(Body::empty())
                    .unwrap(),
            )
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
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/sidecar/tableone?software=SPSS&run_id=run-1")
                    .body(Body::empty())
                    .unwrap(),
            )
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
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/sidecar/does-not-exist?software=R&run_id=run-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "UnknownAlgorithm");
        assert_eq!(v["algorithm_id"], "does-not-exist");
    }

    #[tokio::test]
    async fn returns_400_for_invalid_software_query_token() {
        // axum's Query extractor rejects unknown enum tags with 400;
        // we just confirm the contract surfaces that as 400 rather than
        // reaching the provider.
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.sidecar_provider = Some(Arc::new(FixedProvider(Ok(live_snippet()))));
        let app = build_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/sidecar/tableone?software=Octave&run_id=run-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn returns_503_when_provider_absent() {
        let state = AppState::new(Arc::new(MemSessionStore::new()));
        let app = build_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/sidecar/tableone?software=R&run_id=run-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
