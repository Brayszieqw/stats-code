//! Agent Server library: axum HTTP service for the Stats Web Platform.
//!
//! This library crate exposes the router builder, configuration loader,
//! and middleware so that integration tests can exercise them without
//! starting a TCP listener.

pub mod config;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod orchestrator_adapter;
pub mod state;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, patch, post},
    Router,
};
use middleware::load_shedding::LoadCounter;
use state::AppState;
use tower_http::cors::CorsLayer;

const AUDIO_UPLOAD_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;
// JSON base64 expands a 50 MiB dataset to about 67 MiB before the handler can
// decode and apply the exact 50 MiB dataset limit.
const DATASET_UPLOAD_BODY_LIMIT_BYTES: usize = 70 * 1024 * 1024;

/// Build the application [`Router`] with all routes and middleware layers.
///
/// Middleware execution order (outermost applied last, runs first):
/// 1. CORS (outermost — handles preflight before anything else)
/// 2. Load shedding (tracks concurrency, writes `X-Server-Load` header)
/// 3. Request ID (innermost layer applied first, generates UUID + tracing span)
///
/// Prod 模式下额外为 router 安装一个 SPA fallback：未匹配 `/api/*` 的任意
/// 路径由 [`handlers::static_assets::serve`] 从内嵌的 `web/dist/` 提供（见
/// Requirement 6.2 / 6.3）。`dev-vite` feature 开启时跳过该 fallback，请求
/// 透传给 launcher 启动的 Vite_Dev_Server。
pub fn build_router(load_counter: LoadCounter, app_state: AppState) -> Router {
    let router = Router::new()
        // --- Routes ---
        .route("/api/health", get(health))
        .route("/api/sessions", post(handlers::session::create_session))
        .route("/api/sessions/:sid", get(handlers::session::get_session))
        .route(
            "/api/sessions/:sid/settings",
            patch(handlers::session::patch_settings),
        )
        .route(
            "/api/sessions/:sid/messages",
            post(handlers::message::post_message),
        )
        .route(
            "/api/sessions/:sid/audio",
            post(handlers::audio::post_audio)
                .layer(DefaultBodyLimit::max(AUDIO_UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/sessions/:sid/datasets",
            post(handlers::dataset::post_dataset)
                .layer(DefaultBodyLimit::max(DATASET_UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/sessions/:sid/datasets/:did",
            get(handlers::dataset::get_dataset),
        )
        .route("/api/llm-status", get(handlers::llm_config::get_llm_status))
        .route("/api/llm-config", post(handlers::llm_config::post_llm_config))
        .with_state(app_state);

    // Prod 模式下安装 SPA fallback；dev-vite feature 开启时不安装，
    // 让前端请求落到 Vite_Dev_Server。
    #[cfg(not(feature = "dev-vite"))]
    let router = router.fallback(handlers::static_assets::serve);

    router
        // --- Middleware layers (applied bottom-to-top) ---
        .layer(axum::middleware::from_fn(
            middleware::request_id::request_id,
        ))
        .layer(axum::middleware::from_fn_with_state(
            load_counter.clone(),
            middleware::load_shedding::load_shedding,
        ))
        .layer(CorsLayer::permissive())
}

/// Health check handler — `GET /api/health`.
async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}
