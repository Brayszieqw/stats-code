//! Dataset upload and retrieval HTTP handlers.
//!
//! - `POST /api/sessions/:sid/datasets` → [`post_dataset`]
//! - `GET /api/sessions/:sid/datasets/:did` → [`get_dataset`]
//!
//! Supports two upload modes:
//! 1. `multipart/form-data` with a "file" field
//! 2. JSON body with base64-encoded file content: `{ "filename": "...", "data": "base64..." }`

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use serde::Deserialize;
use uuid::Uuid;

use agent_core::models::{DatasetSummary, ErrorCode, ErrorPayload, SessionStatus};
use agent_core::validation::dataset::{
    check_upload_quota, is_supported_dataset_extension, validate_dataset_non_empty,
    validate_dataset_size,
};

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default per-session upload quota: 200 MB.
const DEFAULT_QUOTA_BYTES: u64 = 200 * 1024 * 1024;

/// Maximum file size: 50 MB (validated before parsing).
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// JSON request body for base64-encoded dataset upload.
#[derive(Debug, Deserialize)]
pub struct Base64DatasetRequest {
    /// Original filename (must have a supported extension).
    pub filename: String,
    /// Base64-encoded file content.
    pub data: String,
}

// ---------------------------------------------------------------------------
// POST /api/sessions/:sid/datasets
// ---------------------------------------------------------------------------

/// `POST /api/sessions/:sid/datasets` — upload a dataset file.
///
/// Accepts either:
/// - `multipart/form-data` with a field named "file"
/// - `application/json` with `{ "filename": "...", "data": "base64..." }`
///
/// Validation steps:
/// 1. Check session exists and is active
/// 2. Validate file extension (csv, tsv, xlsx, xls)
/// 3. Validate file size ≤ 50 MB
/// 4. Check session upload quota (≤ 200 MB total)
/// 5. Save raw file via DatasetStore
/// 6. Parse dataset
/// 7. Validate parsed dataset is non-empty
/// 8. Return DatasetSummary
pub async fn post_dataset(
    State(state): State<AppState>,
    Path(sid): Path<Uuid>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    // 1. Check session exists and is active
    let session_id = agent_core::models::SessionId(sid);
    let session = state
        .session_store
        .get(session_id)
        .await
        .map_err(|e| match e {
            agent_core::traits::session_store::StoreError::NotFound(_) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SessionNotFound,
                    "会话不存在或已被删除",
                ))
            }
            agent_core::traits::session_store::StoreError::Archived => {
                AppError(ErrorPayload::new(
                    ErrorCode::SessionArchived,
                    "会话已归档，仅支持只读访问",
                ))
            }
            agent_core::traits::session_store::StoreError::Internal(msg) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SkillExecutionFailed,
                    format!("内部错误：{msg}"),
                ))
            }
        })?;

    if session.status == SessionStatus::Archived {
        return Err(AppError(ErrorPayload::new(
            ErrorCode::SessionArchived,
            "会话已归档，仅支持只读访问",
        )));
    }

    // 2. Get dataset store
    let dataset_store = state.dataset_store.as_ref().ok_or_else(|| {
        AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            "数据集存储服务尚未初始化",
        ))
    })?;

    // 3. Determine upload mode based on Content-Type
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (filename, file_bytes) = if content_type.starts_with("multipart/form-data") {
        extract_multipart(headers, body).await?
    } else {
        // Default: try JSON base64
        extract_json_base64(&body)?
    };

    // 4. Validate file extension
    if !is_supported_dataset_extension(&filename) {
        return Err(AppError(ErrorPayload::new(
            ErrorCode::DatasetTooLarge,
            "不支持的文件格式，仅支持 csv、tsv、xlsx、xls",
        )));
    }

    // 5. Validate file size ≤ 50 MB (pre-parse check)
    let file_size = file_bytes.len() as u64;
    if file_size > MAX_FILE_SIZE {
        return Err(AppError(ErrorPayload {
            error_code: ErrorCode::DatasetTooLarge,
            message: format!(
                "数据文件过大：文件大小 {} 字节，超过上限 {} 字节",
                file_size, MAX_FILE_SIZE
            ),
            details: None,
        }));
    }

    // 6. Check session upload quota
    let used = dataset_store.quota_used(session_id).await.map_err(|e| {
        AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            format!("查询配额失败：{e}"),
        ))
    })?;
    check_upload_quota(used, file_size, DEFAULT_QUOTA_BYTES).map_err(AppError)?;

    // 7. Save raw file
    let dref = dataset_store
        .save_raw(session_id, &filename, Bytes::from(file_bytes))
        .await
        .map_err(|e| {
            AppError(ErrorPayload::new(
                ErrorCode::SkillExecutionFailed,
                format!("保存文件失败：{e}"),
            ))
        })?;

    // 8. Parse dataset
    let summary = dataset_store.parse(dref).await.map_err(|e| {
        AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            format!("解析数据集失败：{e}"),
        ))
    })?;

    // 9. Validate post-parse: size (row count) and non-empty
    validate_dataset_size(summary.size_bytes, summary.row_count).map_err(AppError)?;
    validate_dataset_non_empty(&summary).map_err(AppError)?;

    // 9.5. Persist the dataset in the session store
    state
        .session_store
        .append_dataset(session_id, summary.clone())
        .await
        .map_err(|e| {
            AppError(ErrorPayload::new(
                ErrorCode::SkillExecutionFailed,
                format!("将数据集信息保存到会话失败：{e}"),
            ))
        })?;

    // 10. Return DatasetSummary
    Ok((StatusCode::CREATED, Json(summary)))
}

// ---------------------------------------------------------------------------
// GET /api/sessions/:sid/datasets/:did
// ---------------------------------------------------------------------------

/// `GET /api/sessions/:sid/datasets/:did` — retrieve a dataset summary.
///
/// Returns the DatasetSummary for a previously uploaded dataset within a session.
pub async fn get_dataset(
    State(state): State<AppState>,
    Path((sid, did)): Path<(Uuid, Uuid)>,
) -> Result<Json<DatasetSummary>, AppError> {
    // 1. Check session exists
    let session_id = agent_core::models::SessionId(sid);
    let session = state
        .session_store
        .get(session_id)
        .await
        .map_err(|e| match e {
            agent_core::traits::session_store::StoreError::NotFound(_) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SessionNotFound,
                    "会话不存在或已被删除",
                ))
            }
            agent_core::traits::session_store::StoreError::Archived => {
                // Archived sessions are still readable
                AppError(ErrorPayload::new(
                    ErrorCode::SessionNotFound,
                    "会话不存在或已被删除",
                ))
            }
            agent_core::traits::session_store::StoreError::Internal(msg) => {
                AppError(ErrorPayload::new(
                    ErrorCode::SkillExecutionFailed,
                    format!("内部错误：{msg}"),
                ))
            }
        })?;

    // 2. Look up dataset in session's datasets list
    let dataset = session
        .datasets
        .iter()
        .find(|d| d.dataset_id == did)
        .ok_or_else(|| {
            AppError(ErrorPayload::new(
                ErrorCode::SessionNotFound,
                "数据集不存在",
            ))
        })?;

    Ok(Json(dataset.clone()))
}

// ---------------------------------------------------------------------------
// Helpers: multipart extraction
// ---------------------------------------------------------------------------

/// Extract filename and bytes from a multipart/form-data request body.
///
/// Looks for a field named "file". Returns error if not found.
async fn extract_multipart(headers: HeaderMap, body: Bytes) -> Result<(String, Vec<u8>), AppError> {
    // Reconstruct the boundary from Content-Type header
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let boundary = multer::parse_boundary(content_type).map_err(|_| {
        AppError(ErrorPayload::new(
            ErrorCode::DatasetTooLarge,
            "无效的 multipart boundary",
        ))
    })?;

    let stream = futures::stream::once(async move { Ok::<_, std::io::Error>(body) });
    let mut multipart = multer::Multipart::new(stream, boundary);

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            format!("读取 multipart 字段失败：{e}"),
        ))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            let filename = field
                .file_name()
                .unwrap_or("unknown.csv")
                .to_string();
            let data = field.bytes().await.map_err(|e| {
                AppError(ErrorPayload::new(
                    ErrorCode::SkillExecutionFailed,
                    format!("读取文件内容失败：{e}"),
                ))
            })?;
            return Ok((filename, data.to_vec()));
        }
    }

    Err(AppError(ErrorPayload::new(
        ErrorCode::DatasetTooLarge,
        "multipart 请求中缺少 'file' 字段",
    )))
}

/// Extract filename and bytes from a JSON base64-encoded request body.
fn extract_json_base64(body: &[u8]) -> Result<(String, Vec<u8>), AppError> {
    use base64::Engine;

    let req: Base64DatasetRequest = serde_json::from_slice(body).map_err(|e| {
        AppError(ErrorPayload::new(
            ErrorCode::SkillExecutionFailed,
            format!("JSON 解析失败：{e}"),
        ))
    })?;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&req.data)
        .map_err(|e| {
            AppError(ErrorPayload::new(
                ErrorCode::SkillExecutionFailed,
                format!("Base64 解码失败：{e}"),
            ))
        })?;

    Ok((req.filename, decoded))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use agent_core::models::{
        ColumnSummary, ColumnType, DatasetRef, DatasetSummary, Encoding, Session, SessionId,
        SessionSettings, SessionStatus,
    };
    use agent_core::traits::dataset_store::DatasetStore;
    use agent_core::traits::session_store::{SessionStore, StoreError};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use base64::Engine;
    use chrono::Utc;
    use std::sync::Arc;
    use tower::ServiceExt;

    // --- Mock SessionStore ---

    struct MockSessionStore {
        session: Option<Session>,
    }

    impl MockSessionStore {
        fn with_active_session(sid: Uuid) -> Self {
            Self {
                session: Some(Session {
                    id: SessionId(sid),
                    status: SessionStatus::Active,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    settings: SessionSettings::default(),
                    messages: vec![],
                    datasets: vec![],
                    skill_runs: vec![],
                    uploaded_bytes: 0,
                }),
            }
        }

        fn with_active_session_and_datasets(sid: Uuid, datasets: Vec<DatasetSummary>) -> Self {
            Self {
                session: Some(Session {
                    id: SessionId(sid),
                    status: SessionStatus::Active,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    settings: SessionSettings::default(),
                    messages: vec![],
                    datasets,
                    skill_runs: vec![],
                    uploaded_bytes: 0,
                }),
            }
        }

        fn with_archived_session(sid: Uuid) -> Self {
            Self {
                session: Some(Session {
                    id: SessionId(sid),
                    status: SessionStatus::Archived,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    settings: SessionSettings::default(),
                    messages: vec![],
                    datasets: vec![],
                    skill_runs: vec![],
                    uploaded_bytes: 0,
                }),
            }
        }

        fn not_found() -> Self {
            Self { session: None }
        }
    }

    #[async_trait::async_trait]
    impl SessionStore for MockSessionStore {
        async fn create(&self) -> Result<Session, StoreError> {
            unimplemented!()
        }
        async fn get(&self, _id: SessionId) -> Result<Session, StoreError> {
            self.session
                .clone()
                .ok_or_else(|| StoreError::NotFound("not found".into()))
        }
        async fn append_message(
            &self,
            _id: SessionId,
            _msg: agent_core::models::Message,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn append_skill_run(
            &self,
            _id: SessionId,
            _run: agent_core::models::SkillRun,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn update_settings(
            &self,
            _id: SessionId,
            _s: SessionSettings,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn archive(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn touch(&self, _id: SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn list_archivable(
            &self,
            _before: chrono::DateTime<Utc>,
        ) -> Result<Vec<SessionId>, StoreError> {
            Ok(vec![])
        }
        async fn append_dataset(
            &self,
            _id: SessionId,
            _dataset: DatasetSummary,
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    // --- Mock DatasetStore ---

    struct MockDatasetStore {
        quota_used: u64,
        parse_result: Option<DatasetSummary>,
    }

    impl MockDatasetStore {
        fn success() -> Self {
            Self {
                quota_used: 0,
                parse_result: Some(DatasetSummary {
                    dataset_id: Uuid::new_v4(),
                    file_name: "test.csv".to_string(),
                    size_bytes: 1024,
                    encoding: Encoding::Utf8,
                    row_count: 10,
                    columns: vec![
                        ColumnSummary {
                            name: "age".to_string(),
                            inferred_type: ColumnType::Numeric,
                            missing_count: 0,
                        },
                        ColumnSummary {
                            name: "name".to_string(),
                            inferred_type: ColumnType::String,
                            missing_count: 1,
                        },
                    ],
                    uploaded_at: Utc::now(),
                }),
            }
        }

        fn with_quota(used: u64) -> Self {
            Self {
                quota_used: used,
                ..Self::success()
            }
        }

        fn empty_dataset() -> Self {
            Self {
                quota_used: 0,
                parse_result: Some(DatasetSummary {
                    dataset_id: Uuid::new_v4(),
                    file_name: "empty.csv".to_string(),
                    size_bytes: 10,
                    encoding: Encoding::Utf8,
                    row_count: 0,
                    columns: vec![],
                    uploaded_at: Utc::now(),
                }),
            }
        }
    }

    #[async_trait::async_trait]
    impl DatasetStore for MockDatasetStore {
        async fn save_raw(
            &self,
            sid: SessionId,
            name: &str,
            _bytes: Bytes,
        ) -> Result<DatasetRef, StoreError> {
            Ok(DatasetRef {
                session_id: sid,
                dataset_id: Uuid::new_v4(),
                raw_path: std::path::PathBuf::from(format!("/tmp/{name}")),
            })
        }

        async fn parse(&self, _dref: DatasetRef) -> Result<DatasetSummary, StoreError> {
            self.parse_result
                .clone()
                .ok_or_else(|| StoreError::Internal("parse failed".into()))
        }

        async fn delete_session_data(&self, _sid: SessionId) -> Result<(), StoreError> {
            Ok(())
        }

        async fn quota_used(&self, _sid: SessionId) -> Result<u64, StoreError> {
            Ok(self.quota_used)
        }
        fn get_path(
            &self,
            _sid: SessionId,
            _dataset_id: uuid::Uuid,
            name: &str,
        ) -> std::path::PathBuf {
            std::path::PathBuf::from(format!("/tmp/{name}"))
        }
    }

    // --- Helper ---

    fn build_test_app(state: AppState) -> Router {
        Router::new()
            .route("/api/sessions/:sid/datasets", post(post_dataset))
            .route("/api/sessions/:sid/datasets/:did", get(get_dataset))
            .with_state(state)
    }

    fn make_state(sid: Uuid, ds: MockDatasetStore) -> AppState {
        let mut state = AppState::new(Arc::new(MockSessionStore::with_active_session(sid)));
        state.dataset_store = Some(Arc::new(ds));
        state
    }

    fn make_json_body(filename: &str, content: &[u8]) -> Vec<u8> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);
        serde_json::to_vec(&serde_json::json!({
            "filename": filename,
            "data": encoded
        }))
        .unwrap()
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_post_dataset_json_base64_success() {
        let sid = Uuid::new_v4();
        let state = make_state(sid, MockDatasetStore::success());
        let app = build_test_app(state);

        let body = make_json_body("data.csv", b"col1,col2\n1,2\n3,4");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let summary: DatasetSummary = serde_json::from_slice(&body).unwrap();
        assert_eq!(summary.row_count, 10);
        assert_eq!(summary.columns.len(), 2);
    }

    #[tokio::test]
    async fn test_post_dataset_unsupported_extension() {
        let sid = Uuid::new_v4();
        let state = make_state(sid, MockDatasetStore::success());
        let app = build_test_app(state);

        let body = make_json_body("data.json", b"{}");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_dataset_session_not_found() {
        let sid = Uuid::new_v4();
        let mut state = AppState::new(Arc::new(MockSessionStore::not_found()));
        state.dataset_store = Some(Arc::new(MockDatasetStore::success()));
        let app = build_test_app(state);

        let body = make_json_body("data.csv", b"col1\n1");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_post_dataset_session_archived() {
        let sid = Uuid::new_v4();
        let mut state = AppState::new(Arc::new(MockSessionStore::with_archived_session(sid)));
        state.dataset_store = Some(Arc::new(MockDatasetStore::success()));
        let app = build_test_app(state);

        let body = make_json_body("data.csv", b"col1\n1");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_post_dataset_quota_exceeded() {
        let sid = Uuid::new_v4();
        // Quota already at 200MB
        let state = make_state(sid, MockDatasetStore::with_quota(200 * 1024 * 1024));
        let app = build_test_app(state);

        let body = make_json_body("data.csv", b"col1\n1");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_post_dataset_empty_dataset_rejected() {
        let sid = Uuid::new_v4();
        let state = make_state(sid, MockDatasetStore::empty_dataset());
        let app = build_test_app(state);

        let body = make_json_body("empty.csv", b"");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // DATASET_EMPTY maps to 422
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_post_dataset_no_dataset_store() {
        let sid = Uuid::new_v4();
        // No dataset_store configured
        let state = AppState::new(Arc::new(MockSessionStore::with_active_session(sid)));
        let app = build_test_app(state);

        let body = make_json_body("data.csv", b"col1\n1");
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/sessions/{sid}/datasets"))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_get_dataset_success() {
        let sid = Uuid::new_v4();
        let did = Uuid::new_v4();
        let dataset = DatasetSummary {
            dataset_id: did,
            file_name: "results.csv".to_string(),
            size_bytes: 2048,
            encoding: Encoding::Utf8,
            row_count: 50,
            columns: vec![ColumnSummary {
                name: "score".to_string(),
                inferred_type: ColumnType::Numeric,
                missing_count: 2,
            }],
            uploaded_at: Utc::now(),
        };

        let mut state = AppState::new(Arc::new(
            MockSessionStore::with_active_session_and_datasets(sid, vec![dataset.clone()]),
        ));
        state.dataset_store = Some(Arc::new(MockDatasetStore::success()));
        let app = build_test_app(state);

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/sessions/{sid}/datasets/{did}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: DatasetSummary = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.dataset_id, did);
        assert_eq!(result.file_name, "results.csv");
    }

    #[tokio::test]
    async fn test_get_dataset_not_found() {
        let sid = Uuid::new_v4();
        let did = Uuid::new_v4();
        let mut state = AppState::new(Arc::new(MockSessionStore::with_active_session(sid)));
        state.dataset_store = Some(Arc::new(MockDatasetStore::success()));
        let app = build_test_app(state);

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/sessions/{sid}/datasets/{did}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_dataset_session_not_found() {
        let sid = Uuid::new_v4();
        let did = Uuid::new_v4();
        let mut state = AppState::new(Arc::new(MockSessionStore::not_found()));
        state.dataset_store = Some(Arc::new(MockDatasetStore::success()));
        let app = build_test_app(state);

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/sessions/{sid}/datasets/{did}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
