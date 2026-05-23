//! `SessionStore` trait definition.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::models::{
    DatasetSummary, Message, Session, SessionId, SessionSettings, SkillRun,
};

/// Errors that can occur in store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The requested entity was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The session is archived and cannot be modified.
    #[error("session is archived")]
    Archived,

    /// An internal/unexpected error occurred.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Async trait for session persistence.
///
/// Implementations handle creating, reading, and updating sessions
/// along with their messages, skill runs, and settings.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create a new session with default settings.
    async fn create(&self) -> Result<Session, StoreError>;

    /// Retrieve a session by ID.
    async fn get(&self, id: SessionId) -> Result<Session, StoreError>;

    /// Append a message to the session history.
    async fn append_message(&self, id: SessionId, msg: Message) -> Result<(), StoreError>;

    /// Append a skill run record to the session.
    async fn append_skill_run(&self, id: SessionId, run: SkillRun) -> Result<(), StoreError>;

    /// Update session settings (e.g., decision assistant toggle).
    async fn update_settings(&self, id: SessionId, s: SessionSettings) -> Result<(), StoreError>;

    /// Mark a session as archived (no further writes allowed).
    async fn archive(&self, id: SessionId) -> Result<(), StoreError>;

    /// Update the `last_active_at` timestamp to the current time.
    async fn touch(&self, id: SessionId) -> Result<(), StoreError>;

    /// List session IDs that have been inactive since before the given timestamp.
    async fn list_archivable(&self, before: DateTime<Utc>) -> Result<Vec<SessionId>, StoreError>;

    /// Append an uploaded dataset summary to the session.
    async fn append_dataset(&self, id: SessionId, dataset: DatasetSummary) -> Result<(), StoreError>;
}

#[async_trait]
impl<T> SessionStore for Arc<T>
where
    T: SessionStore + ?Sized,
{
    async fn create(&self) -> Result<Session, StoreError> {
        self.as_ref().create().await
    }

    async fn get(&self, id: SessionId) -> Result<Session, StoreError> {
        self.as_ref().get(id).await
    }

    async fn append_message(&self, id: SessionId, msg: Message) -> Result<(), StoreError> {
        self.as_ref().append_message(id, msg).await
    }

    async fn append_skill_run(&self, id: SessionId, run: SkillRun) -> Result<(), StoreError> {
        self.as_ref().append_skill_run(id, run).await
    }

    async fn update_settings(&self, id: SessionId, s: SessionSettings) -> Result<(), StoreError> {
        self.as_ref().update_settings(id, s).await
    }

    async fn archive(&self, id: SessionId) -> Result<(), StoreError> {
        self.as_ref().archive(id).await
    }

    async fn touch(&self, id: SessionId) -> Result<(), StoreError> {
        self.as_ref().touch(id).await
    }

    async fn list_archivable(&self, before: DateTime<Utc>) -> Result<Vec<SessionId>, StoreError> {
        self.as_ref().list_archivable(before).await
    }

    async fn append_dataset(&self, id: SessionId, dataset: DatasetSummary) -> Result<(), StoreError> {
        self.as_ref().append_dataset(id, dataset).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemSessionStore;

    #[tokio::test]
    async fn arc_session_store_delegates_to_shared_store() {
        let store: Arc<dyn SessionStore> = Arc::new(MemSessionStore::new());
        let session = store.create().await.expect("create session");
        let fetched = store.get(session.id).await.expect("get session");

        assert_eq!(fetched.id, session.id);
    }
}
