//! In-memory implementation of `SessionStore` for testing and development.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::models::{
    DatasetSummary, Message, Session, SessionId, SessionSettings, SessionStatus, SkillRun,
};
use crate::traits::session_store::{SessionStore, StoreError};

/// In-memory session store backed by a `RwLock<HashMap>`.
///
/// Suitable for tests and single-process development; not durable across restarts.
pub struct MemSessionStore {
    sessions: RwLock<HashMap<SessionId, Session>>,
}

impl MemSessionStore {
    /// Create a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Test-only helper: forcibly set `last_active_at` to an arbitrary instant.
    /// Used to avoid wall-clock `sleep` calls in timing-related tests.
    #[cfg(test)]
    async fn set_last_active_for_test(&self, id: SessionId, when: DateTime<Utc>) {
        let mut map = self.sessions.write().await;
        if let Some(s) = map.get_mut(&id) {
            s.last_active_at = when;
        }
    }
}

impl Default for MemSessionStore {
    fn default() -> Self {
        Self::new()
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
impl SessionStore for MemSessionStore {
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
        let mut map = self.sessions.write().await;
        map.insert(session.id, session.clone());
        Ok(session)
    }

    async fn get(&self, id: SessionId) -> Result<Session, StoreError> {
        let map = self.sessions.read().await;
        map.get(&id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))
    }

    async fn append_message(&self, id: SessionId, msg: Message) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.messages.push(msg);
        session.last_active_at = Utc::now();
        Ok(())
    }

    async fn append_skill_run(&self, id: SessionId, run: SkillRun) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.skill_runs.push(run);
        session.last_active_at = Utc::now();
        Ok(())
    }

    async fn update_settings(&self, id: SessionId, s: SessionSettings) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.settings = s;
        session.last_active_at = Utc::now();
        Ok(())
    }

    async fn archive(&self, id: SessionId) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.status = SessionStatus::Archived;
        Ok(())
    }

    async fn touch(&self, id: SessionId) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.last_active_at = Utc::now();
        Ok(())
    }

    async fn list_archivable(&self, before: DateTime<Utc>) -> Result<Vec<SessionId>, StoreError> {
        let map = self.sessions.read().await;
        let ids: Vec<SessionId> = map
            .values()
            .filter(|s| s.status == SessionStatus::Active && s.last_active_at < before)
            .map(|s| s.id)
            .collect();
        Ok(ids)
    }

    async fn append_dataset(&self, id: SessionId, dataset: DatasetSummary) -> Result<(), StoreError> {
        let mut map = self.sessions.write().await;
        let session = map
            .get_mut(&id)
            .ok_or_else(|| StoreError::NotFound(format!("session {}", id.0)))?;
        reject_if_archived(session)?;
        session.datasets.push(dataset);
        session.last_active_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::models::{Message, UserContent, UserMessage};
    use uuid::Uuid;

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
        let store = MemSessionStore::new();
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
        let store = MemSessionStore::new();
        let result = store.get(SessionId::new()).await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn append_message_to_active_session() {
        let store = MemSessionStore::new();
        let session = store.create().await.unwrap();

        let msg = text_message("hello");
        store.append_message(session.id, msg).await.unwrap();

        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.messages.len(), 1);
    }

    #[tokio::test]
    async fn archive_then_write_returns_archived_error() {
        let store = MemSessionStore::new();
        let session = store.create().await.unwrap();

        // Archive the session
        store.archive(session.id).await.unwrap();

        // Verify status
        let fetched = store.get(session.id).await.unwrap();
        assert_eq!(fetched.status, SessionStatus::Archived);

        // All write operations should fail with Archived
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
        let store = MemSessionStore::new();
        let session = store.create().await.unwrap();
        let original_time = session.last_active_at;

        // Inject an older `last_active_at` rather than sleeping. Avoids flake
        // when sub-millisecond precision varies across platforms.
        let past = original_time - chrono::Duration::seconds(60);
        store.set_last_active_for_test(session.id, past).await;

        store.touch(session.id).await.unwrap();

        let fetched = store.get(session.id).await.unwrap();
        assert!(
            fetched.last_active_at > past,
            "touch must advance last_active_at past the injected baseline"
        );
    }

    #[tokio::test]
    async fn list_archivable_returns_correct_sessions() {
        let store = MemSessionStore::new();

        // Create two sessions
        let s1 = store.create().await.unwrap();
        let s2 = store.create().await.unwrap();

        // Archive s2 so it won't appear in archivable list (only Active sessions qualify)
        store.archive(s2.id).await.unwrap();

        // Use a future timestamp so s1 (Active, last_active_at < before) qualifies
        let future = Utc::now() + Duration::hours(1);
        let archivable = store.list_archivable(future).await.unwrap();

        assert!(archivable.contains(&s1.id));
        assert!(!archivable.contains(&s2.id));

        // Use a past timestamp so nothing qualifies
        let past = Utc::now() - Duration::hours(1);
        let archivable = store.list_archivable(past).await.unwrap();
        assert!(archivable.is_empty());
    }
}
