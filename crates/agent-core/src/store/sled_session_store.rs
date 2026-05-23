//! Sled-backed implementation of `SessionStore` for durable persistence.

use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::models::{
    DatasetSummary, Message, Session, SessionId, SessionSettings, SessionStatus, SkillRun,
};
use crate::traits::session_store::{SessionStore, StoreError};

/// Persistent session store backed by an embedded sled database.
///
/// Sessions are serialized as JSON and keyed by their UUID string representation.
pub struct SledSessionStore {
    db: sled::Db,
}

impl SledSessionStore {
    /// Open (or create) a sled database at the given path.
    ///
    /// # Errors
    /// Returns `StoreError::Internal` if the database cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let db = sled::open(path).map_err(|e| StoreError::Internal(e.to_string()))?;
        Ok(Self { db })
    }

    /// Serialize a session to JSON bytes.
    fn serialize(session: &Session) -> Result<Vec<u8>, StoreError> {
        serde_json::to_vec(session).map_err(|e| StoreError::Internal(e.to_string()))
    }

    /// Deserialize a session from JSON bytes.
    fn deserialize(bytes: &[u8]) -> Result<Session, StoreError> {
        serde_json::from_slice(bytes).map_err(|e| StoreError::Internal(e.to_string()))
    }

    /// Convert a `SessionId` to the sled key bytes.
    fn key(id: SessionId) -> Vec<u8> {
        id.0.to_string().into_bytes()
    }

    /// Load a session by ID from the database.
    fn load(&self, id: SessionId) -> Result<Session, StoreError> {
        let key = Self::key(id);
        let bytes = self
            .db
            .get(&key)
            .map_err(|e| StoreError::Internal(e.to_string()))?
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        Self::deserialize(&bytes)
    }

    /// Save a session to the database.
    fn save(&self, session: &Session) -> Result<(), StoreError> {
        let key = Self::key(session.id);
        let value = Self::serialize(session)?;
        self.db
            .insert(key, value)
            .map_err(|e| StoreError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Test-only helper: forcibly set `last_active_at` to an arbitrary instant.
    /// Used to avoid wall-clock `sleep` calls in timing-related tests.
    #[cfg(test)]
    fn set_last_active_for_test(&self, id: SessionId, when: DateTime<Utc>) -> Result<(), StoreError> {
        let mut s = self.load(id)?;
        s.last_active_at = when;
        self.save(&s)
    }
}

/// Helper: returns `Err(StoreError::Archived)` if the session is archived.
fn reject_if_archived(session: &Session) -> Result<(), StoreError> {
    if session.status == SessionStatus::Archived {
        return Err(StoreError::Archived);
    }
    Ok(())
}

#[async_trait]
impl SessionStore for SledSessionStore {
    async fn create(&self) -> Result<Session, StoreError> {
        let now = Utc::now();
        let session = Session {
            id: SessionId::new(),
            status: SessionStatus::Active,
            created_at: now,
            last_active_at: now,
            settings: SessionSettings::default(),
            messages: Vec::new(),
            datasets: Vec::new(),
            skill_runs: Vec::new(),
            uploaded_bytes: 0,
        };
        self.save(&session)?;
        Ok(session)
    }

    async fn get(&self, id: SessionId) -> Result<Session, StoreError> {
        self.load(id)
    }

    async fn append_message(&self, id: SessionId, msg: Message) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.messages.push(msg);
        session.last_active_at = Utc::now();
        self.save(&session)
    }

    async fn append_skill_run(&self, id: SessionId, run: SkillRun) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.skill_runs.push(run);
        session.last_active_at = Utc::now();
        self.save(&session)
    }

    async fn update_settings(&self, id: SessionId, s: SessionSettings) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.settings = s;
        session.last_active_at = Utc::now();
        self.save(&session)
    }

    async fn archive(&self, id: SessionId) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.status = SessionStatus::Archived;
        self.save(&session)
    }

    async fn touch(&self, id: SessionId) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.last_active_at = Utc::now();
        self.save(&session)
    }

    async fn list_archivable(&self, before: DateTime<Utc>) -> Result<Vec<SessionId>, StoreError> {
        let mut ids = Vec::new();
        for entry in self.db.iter() {
            let (_key, value) =
                entry.map_err(|e| StoreError::Internal(e.to_string()))?;
            let session = Self::deserialize(&value)?;
            if session.status == SessionStatus::Active && session.last_active_at < before {
                ids.push(session.id);
            }
        }
        Ok(ids)
    }

    async fn append_dataset(&self, id: SessionId, dataset: DatasetSummary) -> Result<(), StoreError> {
        let mut session = self.load(id)?;
        reject_if_archived(&session)?;
        session.datasets.push(dataset);
        session.last_active_at = Utc::now();
        self.save(&session)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Message, UserContent, UserMessage};
    use chrono::Duration;
    use tempfile::tempdir;
    use uuid::Uuid;

    /// Helper to create a `SledSessionStore` in a temporary directory.
    fn temp_store() -> SledSessionStore {
        let dir = tempdir().unwrap();
        SledSessionStore::open(dir.path().join("test_db")).unwrap()
    }

    /// Helper to create a simple user text message.
    fn text_message(text: &str) -> Message {
        Message::User(UserMessage {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            content: UserContent::Text(text.to_string()),
        })
    }

    #[tokio::test]
    async fn create_and_get_session() {
        let store = temp_store();
        let session = store.create().await.unwrap();

        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.messages.is_empty());
        assert!(session.settings.decision_assistant);

        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.status, SessionStatus::Active);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_not_found() {
        let store = temp_store();
        let result = store.get(SessionId::new()).await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn append_message_persists() {
        let store = temp_store();
        let session = store.create().await.unwrap();

        let msg = text_message("hello sled");
        store.append_message(session.id, msg).await.unwrap();

        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.messages.len(), 1);
    }

    #[tokio::test]
    async fn archive_then_write_returns_archived_error() {
        let store = temp_store();
        let session = store.create().await.unwrap();

        store.archive(session.id).await.unwrap();

        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.status, SessionStatus::Archived);

        // Write operations should fail
        let msg = text_message("should fail");
        let result = store.append_message(session.id, msg).await;
        assert!(matches!(result, Err(StoreError::Archived)));

        let result = store
            .update_settings(session.id, SessionSettings { decision_assistant: false })
            .await;
        assert!(matches!(result, Err(StoreError::Archived)));

        let result = store.touch(session.id).await;
        assert!(matches!(result, Err(StoreError::Archived)));

        // Archive again should also fail
        let result = store.archive(session.id).await;
        assert!(matches!(result, Err(StoreError::Archived)));

        // Read still works
        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.status, SessionStatus::Archived);
    }

    #[tokio::test]
    async fn touch_updates_last_active_at() {
        let store = temp_store();
        let session = store.create().await.unwrap();
        let original_time = session.last_active_at;

        // Inject an older `last_active_at` rather than sleeping.
        let past = original_time - chrono::Duration::seconds(60);
        store.set_last_active_for_test(session.id, past).unwrap();

        store.touch(session.id).await.unwrap();

        let fetched = store.get(session.id).await.unwrap();
        assert!(
            fetched.last_active_at > past,
            "touch must advance last_active_at past the injected baseline"
        );
    }

    #[tokio::test]
    async fn list_archivable_returns_correct_sessions() {
        let store = temp_store();

        let s1 = store.create().await.unwrap();
        let s2 = store.create().await.unwrap();

        // Archive s2 so it won't appear in archivable list
        store.archive(s2.id).await.unwrap();

        // Future timestamp: s1 (Active, last_active_at < before) qualifies
        let future = Utc::now() + Duration::hours(1);
        let archivable = store.list_archivable(future).await.unwrap();

        assert!(archivable.contains(&s1.id));
        assert!(!archivable.contains(&s2.id));

        // Past timestamp: nothing qualifies
        let past = Utc::now() - Duration::hours(1);
        let archivable = store.list_archivable(past).await.unwrap();
        assert!(archivable.is_empty());
    }

    #[tokio::test]
    async fn data_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("persist_db");

        let session_id;
        {
            let store = SledSessionStore::open(&db_path).unwrap();
            let session = store.create().await.unwrap();
            session_id = session.id;
            let msg = text_message("persistent message");
            store.append_message(session_id, msg).await.unwrap();
        }

        // Reopen the database and verify data is still there
        let store = SledSessionStore::open(&db_path).unwrap();
        let fetched = store.get(session_id).await.unwrap();
        assert_eq!(fetched.messages.len(), 1);
    }
}
