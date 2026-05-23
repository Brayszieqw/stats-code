//! Core data models for the agent platform.

pub mod dataset;
pub mod error;
pub mod llm_config;
pub mod message;
pub mod session;
pub mod skill;

// Re-export key types for convenience.
pub use dataset::{ColumnSummary, ColumnType, DatasetId, DatasetRef, DatasetSummary, Encoding};
pub use error::{http_status_for, ErrorCode, ErrorPayload, ALL_ERROR_CODES};
pub use llm_config::{LlmConfig, LlmProvider};
pub use message::{
    AgentBlock, AgentMessage, ChoiceAnswer, ChoiceOption, ChoicePrompt, Message, MessageId,
    OptionId, PromptId, UserContent, UserMessage,
};
pub use session::{Session, SessionId, SessionSettings, SessionStatus};
pub use skill::{RiskSignal, SkillError, SkillOutcome, SkillResult, SkillRun, SkillRunId};
