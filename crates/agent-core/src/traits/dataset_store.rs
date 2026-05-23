//! `DatasetStore` trait definition.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::models::{DatasetId, DatasetRef, DatasetSummary, SessionId};

use super::session_store::StoreError;

/// Async trait for dataset file persistence and parsing.
///
/// Implementations handle saving raw uploaded files, parsing them into
/// structured summaries, cleaning up session data, and tracking quota usage.
#[async_trait]
pub trait DatasetStore: Send + Sync {
    /// Save a raw uploaded file and return a reference to it.
    async fn save_raw(
        &self,
        sid: SessionId,
        name: &str,
        bytes: Bytes,
    ) -> Result<DatasetRef, StoreError>;

    /// Parse a previously saved raw file into a structured summary.
    async fn parse(&self, dref: DatasetRef) -> Result<DatasetSummary, StoreError>;

    /// Delete all dataset files associated with a session.
    async fn delete_session_data(&self, sid: SessionId) -> Result<(), StoreError>;

    /// Return the total bytes used by datasets in a session.
    async fn quota_used(&self, sid: SessionId) -> Result<u64, StoreError>;

    /// Get the physical file path of a dataset.
    fn get_path(&self, sid: SessionId, dataset_id: DatasetId, name: &str) -> std::path::PathBuf;
}

#[async_trait]
impl<T> DatasetStore for Arc<T>
where
    T: DatasetStore + ?Sized,
{
    async fn save_raw(
        &self,
        sid: SessionId,
        name: &str,
        bytes: Bytes,
    ) -> Result<DatasetRef, StoreError> {
        self.as_ref().save_raw(sid, name, bytes).await
    }

    async fn parse(&self, dref: DatasetRef) -> Result<DatasetSummary, StoreError> {
        self.as_ref().parse(dref).await
    }

    async fn delete_session_data(&self, sid: SessionId) -> Result<(), StoreError> {
        self.as_ref().delete_session_data(sid).await
    }

    async fn quota_used(&self, sid: SessionId) -> Result<u64, StoreError> {
        self.as_ref().quota_used(sid).await
    }

    fn get_path(&self, sid: SessionId, dataset_id: DatasetId, name: &str) -> std::path::PathBuf {
        self.as_ref().get_path(sid, dataset_id, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FsDatasetStore;

    #[tokio::test]
    async fn arc_dataset_store_delegates_to_shared_store() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn DatasetStore> =
            Arc::new(FsDatasetStore::new(tmp.path()).await.expect("dataset store"));
        let sid = SessionId::new();

        let dref = store
            .save_raw(sid, "data.csv", Bytes::from_static(b"value\n1\n"))
            .await
            .expect("save raw");
        let summary = store.parse(dref).await.expect("parse");

        assert_eq!(summary.file_name, "data.csv");
        assert_eq!(summary.row_count, 1);
        assert!(store.quota_used(sid).await.expect("quota") > 0);
    }
}
