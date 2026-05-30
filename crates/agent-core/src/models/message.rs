//! Message domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::skill::{SkillResult, SkillRunId};

/// Type alias for message identifiers.
pub type MessageId = Uuid;

/// Type alias for choice prompt identifiers.
pub type PromptId = Uuid;

/// Type alias for choice option identifiers.
pub type OptionId = String;

/// A message in the session history (either user or agent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    User(UserMessage),
    Agent(AgentMessage),
}

/// A message sent by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub id: MessageId,
    pub created_at: DateTime<Utc>,
    pub content: UserContent,
}

/// The content of a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserContent {
    /// Plain text input (length ≤ 8000 chars, R1.4).
    Text(String),
    /// Transcribed audio input with confidence score.
    AudioTranscript { text: String, confidence: f32 },
    /// Answer to a `ChoicePrompt`.
    ChoiceAnswer {
        prompt_id: PromptId,
        options: Vec<OptionId>,
        custom_text: Option<String>,
    },
}

/// A message sent by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: MessageId,
    pub created_at: DateTime<Utc>,
    pub blocks: Vec<AgentBlock>,
}

/// A block within an agent message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentBlock {
    /// Free-form text response.
    Text(String),
    /// A structured choice prompt for the user.
    ChoicePrompt(ChoicePrompt),
    /// Result of a skill execution.
    SkillResult {
        run_id: SkillRunId,
        result: SkillResult,
    },
    /// AI interpretation of a skill result.
    Interpretation(String),
}

/// A structured choice prompt sent to the user (R4.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoicePrompt {
    pub prompt_id: PromptId,
    pub question: String,
    pub options: Vec<ChoiceOption>,
    pub multi_select: bool,
    pub allow_custom_text: bool,
    /// Recommended option (must be present in `options` if `Some`, R5.5).
    pub recommendation: Option<OptionId>,
}

/// A single option within a choice prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub option_id: OptionId,
    pub text: String,
    /// Optional brief explanation of this option.
    pub explanation: Option<String>,
}

/// A user's answer to a `ChoicePrompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceAnswer {
    pub prompt_id: PromptId,
    pub options: Vec<OptionId>,
    pub custom_text: Option<String>,
}
