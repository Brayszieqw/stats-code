//! Request ID middleware.
//!
//! Generates a UUID v4 for every incoming request, attaches it to a tracing span,
//! and writes the value into the `X-Request-Id` response header.

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// Header name for the request ID.
pub const X_REQUEST_ID: &str = "x-request-id";

/// Middleware that assigns a unique `X-Request-Id` to every request/response
/// and records it in the current tracing span.
pub async fn request_id(req: Request, next: Next) -> Response {
    let id = Uuid::new_v4().to_string();

    // Record the request_id in the current tracing span so log entries correlate.
    tracing::Span::current().record("request_id", id.as_str());
    let span = tracing::info_span!("request", request_id = %id);
    let _guard = span.enter();

    let mut response = next.run(req).await;

    // Attach the header to the outgoing response.
    if let Ok(val) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(X_REQUEST_ID, val);
    }

    response
}
