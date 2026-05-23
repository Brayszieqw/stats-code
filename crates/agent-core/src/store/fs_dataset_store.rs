//! Filesystem-backed implementation of `DatasetStore`.
//!
//! Saves raw uploads under `<root>/<session_id>/<dataset_id>__<filename>` and
//! parses CSV / TSV via the `csv` crate, falling back to a minimal byte counter
//! for binary formats (xlsx / xls) which agent-server's parser will refine later.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use tokio::fs;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::encoding::detect_and_decode;
use crate::models::{
    ColumnSummary, ColumnType, DatasetRef, DatasetSummary, Encoding, SessionId,
};
use crate::traits::dataset_store::DatasetStore;
use crate::traits::session_store::StoreError;

/// Filesystem-backed dataset store.
///
/// Layout:
/// ```text
/// <root>/
///   <session_id_uuid>/
///     <dataset_id_uuid>__<original_filename>
/// ```
///
/// Quota is tracked in-memory (best-effort); restart re-scans the directory.
pub struct FsDatasetStore {
    root: PathBuf,
    inner: Arc<Mutex<()>>,
}

impl FsDatasetStore {
    /// Create the store rooted at `root`. The directory will be created if absent.
    pub async fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root)
            .await
            .map_err(|e| StoreError::Internal(format!("create dataset root: {e}")))?;
        Ok(Self {
            root,
            inner: Arc::new(Mutex::new(())),
        })
    }

    fn session_dir(&self, sid: SessionId) -> PathBuf {
        self.root.join(sid.0.to_string())
    }

    fn build_path(&self, sid: SessionId, dataset_id: Uuid, name: &str) -> PathBuf {
        self.session_dir(sid)
            .join(format!("{}__{}", dataset_id, sanitize_filename(name)))
    }
}

/// Strip path separators and control characters from a filename.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[async_trait]
impl DatasetStore for FsDatasetStore {
    async fn save_raw(
        &self,
        sid: SessionId,
        name: &str,
        bytes: Bytes,
    ) -> Result<DatasetRef, StoreError> {
        let _guard = self.inner.lock().await;
        let dir = self.session_dir(sid);
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| StoreError::Internal(format!("create session dir: {e}")))?;

        let dataset_id = Uuid::new_v4();
        let path = self.build_path(sid, dataset_id, name);
        fs::write(&path, &bytes)
            .await
            .map_err(|e| StoreError::Internal(format!("write dataset file: {e}")))?;

        Ok(DatasetRef {
            session_id: sid,
            dataset_id,
            raw_path: path,
        })
    }

    async fn parse(&self, dref: DatasetRef) -> Result<DatasetSummary, StoreError> {
        let bytes = fs::read(&dref.raw_path)
            .await
            .map_err(|e| StoreError::Internal(format!("read dataset file: {e}")))?;

        let size_bytes = bytes.len() as u64;
        let file_name = dref
            .raw_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| {
                // Strip the "<dataset_id>__" prefix we added in save_raw.
                if let Some(idx) = s.find("__") {
                    s[idx + 2..].to_string()
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        let extension = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());

        let summary = match extension.as_deref() {
            Some("csv") | Some("tsv") => parse_text_table(&bytes, dref.dataset_id, file_name, extension.as_deref().unwrap_or("csv"))?,
            Some("xlsx") | Some("xls") => DatasetSummary {
                dataset_id: dref.dataset_id,
                file_name,
                size_bytes,
                encoding: Encoding::Utf8,
                row_count: 0,
                columns: Vec::new(),
                uploaded_at: Utc::now(),
            },
            _ => {
                return Err(StoreError::Internal(format!(
                    "unsupported file extension: {extension:?}"
                )));
            }
        };

        Ok(summary)
    }

    async fn delete_session_data(&self, sid: SessionId) -> Result<(), StoreError> {
        let _guard = self.inner.lock().await;
        let dir = self.session_dir(sid);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .await
                .map_err(|e| StoreError::Internal(format!("remove session dir: {e}")))?;
        }
        Ok(())
    }

    async fn quota_used(&self, sid: SessionId) -> Result<u64, StoreError> {
        let _guard = self.inner.lock().await;
        let dir = self.session_dir(sid);
        if !dir.exists() {
            return Ok(0);
        }
        let mut total = 0u64;
        let mut entries = fs::read_dir(&dir)
            .await
            .map_err(|e| StoreError::Internal(format!("read session dir: {e}")))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StoreError::Internal(format!("iter session dir: {e}")))?
        {
            let meta = entry
                .metadata()
                .await
                .map_err(|e| StoreError::Internal(format!("stat: {e}")))?;
            if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
        Ok(total)
    }

    fn get_path(&self, sid: SessionId, dataset_id: Uuid, name: &str) -> PathBuf {
        self.build_path(sid, dataset_id, name)
    }
}

/// Parse a CSV / TSV byte slice into a `DatasetSummary`.
fn parse_text_table(
    bytes: &[u8],
    dataset_id: Uuid,
    file_name: String,
    ext: &str,
) -> Result<DatasetSummary, StoreError> {
    let (decoded, encoding) =
        detect_and_decode(bytes).map_err(|e| StoreError::Internal(format!("decode: {e}")))?;

    let delimiter = if ext == "tsv" { b'\t' } else { b',' };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(decoded.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| StoreError::Internal(format!("read headers: {e}")))?
        .clone();

    let column_count = headers.len();
    let mut missing_counts = vec![0u64; column_count];
    let mut numeric_counts = vec![0u64; column_count];
    let mut row_count: u64 = 0;

    for record in reader.records() {
        let record = record.map_err(|e| StoreError::Internal(format!("read row: {e}")))?;
        row_count = row_count.saturating_add(1);
        for (idx, field) in record.iter().enumerate() {
            if idx >= column_count {
                break;
            }
            let trimmed = field.trim();
            if trimmed.is_empty() {
                missing_counts[idx] = missing_counts[idx].saturating_add(1);
                continue;
            }
            if trimmed.parse::<f64>().is_ok() {
                numeric_counts[idx] = numeric_counts[idx].saturating_add(1);
            }
        }
    }

    let columns: Vec<ColumnSummary> = headers
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let non_missing = row_count.saturating_sub(missing_counts[idx]);
            let inferred_type = if non_missing > 0 && numeric_counts[idx] == non_missing {
                ColumnType::Numeric
            } else {
                ColumnType::String
            };
            ColumnSummary {
                name: name.to_string(),
                inferred_type,
                missing_count: missing_counts[idx],
            }
        })
        .collect();

    Ok(DatasetSummary {
        dataset_id,
        file_name,
        size_bytes: bytes.len() as u64,
        encoding,
        row_count,
        columns,
        uploaded_at: Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_sid() -> SessionId {
        SessionId(Uuid::new_v4())
    }

    #[tokio::test]
    async fn save_and_parse_csv() {
        let tmp = TempDir::new().unwrap();
        let store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let sid = new_sid();

        let csv = b"a,b\n1,2\n3,4\n";
        let dref = store
            .save_raw(sid, "data.csv", Bytes::from_static(csv))
            .await
            .unwrap();

        let summary = store.parse(dref).await.unwrap();
        assert_eq!(summary.file_name, "data.csv");
        assert_eq!(summary.row_count, 2);
        assert_eq!(summary.columns.len(), 2);
        assert_eq!(summary.columns[0].name, "a");
        assert!(matches!(summary.columns[0].inferred_type, ColumnType::Numeric));
    }

    #[tokio::test]
    async fn quota_tracks_uploaded_bytes() {
        let tmp = TempDir::new().unwrap();
        let store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let sid = new_sid();

        assert_eq!(store.quota_used(sid).await.unwrap(), 0);

        let payload = vec![0u8; 1024];
        let _ = store
            .save_raw(sid, "x.csv", Bytes::from(payload))
            .await
            .unwrap();
        assert_eq!(store.quota_used(sid).await.unwrap(), 1024);
    }

    #[tokio::test]
    async fn delete_session_data_removes_files() {
        let tmp = TempDir::new().unwrap();
        let store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let sid = new_sid();

        let _ = store
            .save_raw(sid, "a.csv", Bytes::from_static(b"x,y\n1,2\n"))
            .await
            .unwrap();

        store.delete_session_data(sid).await.unwrap();
        assert_eq!(store.quota_used(sid).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn sanitize_filename_strips_separators() {
        // Each path separator becomes a single '_'.
        assert_eq!(sanitize_filename("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_filename("normal.csv"), "normal.csv");
        assert_eq!(sanitize_filename("a\\b\\c.csv"), "a_b_c.csv");
    }

    #[tokio::test]
    async fn parse_tsv_uses_tab_delimiter() {
        let tmp = TempDir::new().unwrap();
        let store = FsDatasetStore::new(tmp.path().to_path_buf()).await.unwrap();
        let sid = new_sid();

        let tsv = b"col1\tcol2\nfoo\t1\nbar\t2\n";
        let dref = store
            .save_raw(sid, "data.tsv", Bytes::from_static(tsv))
            .await
            .unwrap();

        let summary = store.parse(dref).await.unwrap();
        assert_eq!(summary.columns.len(), 2);
        assert_eq!(summary.columns[0].name, "col1");
        assert_eq!(summary.columns[1].name, "col2");
    }
}
