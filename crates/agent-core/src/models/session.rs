//! Session domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::dataset::DatasetSummary;
use super::message::Message;
use super::skill::SkillRun;

/// Strongly-typed session identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new random `SessionId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Session lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Archived,
}

/// Per-session user settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    /// Whether the decision assistant mode is enabled (default: true, R5.1).
    pub decision_assistant: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            decision_assistant: true,
        }
    }
}

/// A complete session containing messages, datasets, and skill runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub settings: SessionSettings,
    pub messages: Vec<Message>,
    pub datasets: Vec<DatasetSummary>,
    pub skill_runs: Vec<SkillRun>,
    /// Cumulative bytes uploaded in this session (for quota enforcement, R13.4).
    pub uploaded_bytes: u64,
}
