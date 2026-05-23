//! Error response conversion for axum.
//!
//! Provides [`AppError`], a newtype wrapper around [`ErrorPayload`] that implements
//! axum's `IntoResponse` trait. This satisfies Rust's orphan rule (both `IntoResponse`
//! and `ErrorPayload` are defined in external crates) while producing JSON responses
//! with the correct HTTP status code and `Content-Type: application/json` header.

use agent_core::models::ErrorPayload;
use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

/// Newtype wrapper enabling `IntoResponse` for [`ErrorPayload`].
///
/// # Response format
///
/// - HTTP status code: determined by [`agent_core::models::http_status_for`]
/// - Content-Type: `application/json`
/// - Body: `{ "error_code": "...", "message": "...", "details": ... }`
#[derive(Debug)]
pub struct AppError(pub ErrorPayload);

impl From<ErrorPayload> for AppError {
    fn from(payload: ErrorPayload) -> Self {
        Self(payload)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status_u16, body) = self.0.to_http_parts();
        let status = StatusCode::from_u16(status_u16).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        (
            status,
            [(header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response()
    }
}
