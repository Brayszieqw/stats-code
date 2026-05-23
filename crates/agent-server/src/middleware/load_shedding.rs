//! Load-shedding middleware.
//!
//! Tracks the number of in-flight requests. When the concurrent count exceeds
//! the configured threshold (default 50), the response carries
//! `X-Server-Load: degraded`. Requests are **never** rejected with 503
//! regardless of load (R12.5).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

/// Header name indicating server load status.
pub const X_SERVER_LOAD: &str = "x-server-load";

/// Default concurrency threshold above which we signal degraded mode.
const DEFAULT_THRESHOLD: u32 = 50;

/// Shared counter for in-flight requests.
#[derive(Debug, Clone)]
pub struct LoadCounter {
    inflight: Arc<AtomicU32>,
    threshold: u32,
}

impl LoadCounter {
    /// Create a new counter with the given concurrency threshold.
    #[must_use]
    pub fn new(threshold: u32) -> Self {
        Self {
            inflight: Arc::new(AtomicU32::new(0)),
            threshold,
        }
    }

    /// Increment the in-flight count. Returns the new value.
    fn acquire(&self) -> u32 {
        self.inflight.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement the in-flight count.
    fn release(&self) {
        self.inflight.fetch_sub(1, Ordering::SeqCst);
    }

    /// Returns `true` if current load exceeds the threshold.
    fn is_degraded(&self, current: u32) -> bool {
        current > self.threshold
    }
}

impl Default for LoadCounter {
    fn default() -> Self {
        Self::new(DEFAULT_THRESHOLD)
    }
}

/// Middleware that tracks concurrency and signals degraded mode via response header.
///
/// Must be used with `axum::middleware::from_fn_with_state` passing a [`LoadCounter`].
pub async fn load_shedding(
    axum::extract::State(counter): axum::extract::State<LoadCounter>,
    req: Request,
    next: Next,
) -> Response {
    let current = counter.acquire();
    let degraded = counter.is_degraded(current);

    let mut response = next.run(req).await;

    if degraded {
        response
            .headers_mut()
            .insert(X_SERVER_LOAD, HeaderValue::from_static("degraded"));
    }

    counter.release();
    response
}
