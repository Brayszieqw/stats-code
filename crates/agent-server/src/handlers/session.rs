//! Session-related HTTP handlers.
//!
//! - `POST /api/sessions` → [`create_session`]
//! - `GET /api/sessions/:sid` → [`get_session`]
//! - `PATCH /api/sessions/:sid/settings` → [`patch_settings`]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use agent_core::models::{ErrorCode, ErrorPayload, Session, SessionSettings, SessionStatus};
use agent_core::traits::session_store::StoreError;

use crate::error::AppError;
use crate::state::AppState;

/// `POST /api/sessions` — create a new session.
///
/// Returns 201 Created with the newly created session as JSON body.
pub async fn create_session(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let session = state.session_store.create().await.map_err(store_error_to_app)?;
    Ok((StatusCode::CREATED, Json(session)))
}

/// `GET /api/sessions/:sid` — retrieve a session by ID.
///
/// Returns 200 OK with the session, or 404 if not found.
pub async fn get_session(
    State(state): State<AppState>,
    Path(sid): Path<Uuid>,
) -> Result<Json<Session>, AppError> {
    let session_id = agent_core::models::SessionId(sid);
    let session = state
        .session_store
        .get(session_id)
        .await
        .map_err(store_error_to_app)?;
    Ok(Json(session))
}

/// Request body for `PATCH /api/sessions/:sid/settings`.
#[derive(Debug, serde::Deserialize)]
pub struct PatchSettingsRequest {
    /// Whether the decision assistant mode is enabled.
    pub decision_assistant: bool,
}

/// `PATCH /api/sessions/:sid/settings` — update session settings.
///
/// Returns 200 OK with the updated session, or:
/// - 404 if session not found
/// - 409 if session is archived
pub async fn patch_settings(
    State(state): State<AppState>,
    Path(sid): Path<Uuid>,
    Json(body): Json<PatchSettingsRequest>,
) -> Result<Json<Session>, AppError> {
    let session_id = agent_core::models::SessionId(sid);

    // First check existence (get returns NotFound if absent)
    let session = state
        .session_store
        .get(session_id)
        .await
        .map_err(store_error_to_app)?;

    // Check if archived — write operations on archived sessions return 409
    if session.status == SessionStatus::Archived {
        return Err(AppError(ErrorPayload::new(
            ErrorCode::SessionArchived,
            "会话已归档，仅支持只读访问",
        )));
    }

    let new_settings = SessionSettings {
        decision_assistant: body.decision_assistant,
    };

    state
        .session_store
        .update_settings(session_id, new_settings)
        .await
        .map_err(store_error_to_app)?;

    // Return the updated session
    let updated = state
        .session_store
        .get(session_id)
        .await
        .map_err(store_error_to_app)?;
    Ok(Json(updated))
}

/// Convert a [`StoreError`] into an [`AppError`] with the appropriate error code.
fn store_error_to_app(err: StoreError) -> AppError {
    match err {
        StoreError::NotFound(_) => AppError(ErrorPayload::new(
            ErrorCode::SessionNotFound,
            "会话不存在或已被删除",
        )),
        StoreError::Archived => AppError(ErrorPayload::new(
            ErrorCode::SessionArchived,
            "会话已归档，仅支持只读访问",
        )),
        StoreError::Internal(msg) => AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            format!("内部错误：{msg}"),
        )),
    }
}
