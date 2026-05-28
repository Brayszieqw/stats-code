//! `GET /api/coverage-matrix` — serializes the embedded Algorithm Coverage
//! Matrix as JSON for the SPA's `coverageMatrix.ts` client (task 10.2).
//!
//! The matrix is read from [`AppState::coverage_matrix_provider`]. The
//! launcher in `stats-code` injects a concrete provider whose `get`
//! returns a clone of the once-loaded `CoverageMatrix` DTO. When the
//! provider is absent (e.g. during early-boot tests) the handler returns
//! 503 Service Unavailable rather than fabricating an empty matrix —
//! callers should never observe a `[]` response that looks valid.
//!
//! Validates: Requirements 6.2 (matrix transport), 10.3 (no new ports —
//! the route lives on the existing axum router).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::state::AppState;

/// `GET /api/coverage-matrix` handler.
///
/// - 200 + `CoverageMatrixDto` JSON when a provider is configured.
/// - 503 + structured error body when the provider is absent.
pub async fn get_coverage_matrix(State(state): State<AppState>) -> Response {
    match state.coverage_matrix_provider.as_ref() {
        Some(provider) => (StatusCode::OK, Json(provider.get())).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error_code": "CoverageMatrixUnavailable",
                "message": "coverage matrix provider not configured",
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use agent_core::store::MemSessionStore;
    use api::sidecar::{
        AlgorithmEntryDto, CoverageMatrixDto, CoverageValueDto, ReferenceImplDto,
        ReferenceSoftware,
    };
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use crate::state::CoverageMatrixProvider;

    struct FixedProvider(CoverageMatrixDto);

    impl CoverageMatrixProvider for FixedProvider {
        fn get(&self) -> CoverageMatrixDto {
            self.0.clone()
        }
    }

    fn fixture_matrix() -> CoverageMatrixDto {
        let mut coverage = BTreeMap::new();
        coverage.insert(ReferenceSoftware::R, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SAS, CoverageValueDto::Recorded);
        coverage.insert(ReferenceSoftware::Python, CoverageValueDto::Live);
        coverage.insert(ReferenceSoftware::SPSS, CoverageValueDto::None_);

        let mut reference = BTreeMap::new();
        reference.insert(
            ReferenceSoftware::R,
            ReferenceImplDto {
                callable: "tableone::CreateTableOne".into(),
                package: Some("tableone".into()),
                version: "0.13.2".into(),
            },
        );

        CoverageMatrixDto {
            schema_version: 1,
            release_version: "0.5.0".into(),
            algorithms: vec![AlgorithmEntryDto {
                id: "tableone".into(),
                display_name: "Table One".into(),
                iterative: false,
                coverage,
                reference,
            }],
        }
    }

    fn build_app(state: AppState) -> Router {
        Router::new()
            .route("/api/coverage-matrix", get(get_coverage_matrix))
            .with_state(state)
    }

    #[tokio::test]
    async fn returns_200_with_matrix_when_provider_present() {
        let mut state = AppState::new(Arc::new(MemSessionStore::new()));
        state.coverage_matrix_provider = Some(Arc::new(FixedProvider(fixture_matrix())));
        let app = build_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/coverage-matrix")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let dto: CoverageMatrixDto = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(dto, fixture_matrix());
    }

    #[tokio::test]
    async fn returns_503_when_provider_absent() {
        let state = AppState::new(Arc::new(MemSessionStore::new()));
        let app = build_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/coverage-matrix")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error_code"], "CoverageMatrixUnavailable");
    }
}
