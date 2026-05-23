//! `LlmProvider` trait definition.

use std::pin::Pin;

use async_trait::async_trait;
use thiserror::Error;
use tokio_stream::Stream;

/// A request to the LLM provider.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// The conversation messages to send.
    pub messages: Vec<LlmMessage>,
    /// Model identifier (e.g., "deepseek-chat").
    pub model: String,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Temperature for sampling.
    pub temperature: Option<f32>,
}

/// A single message in the LLM conversation.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

/// Role of a message in the LLM conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

/// Events emitted by the LLM stream.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    /// A chunk of generated text.
    TextDelta(String),
    /// The stream has completed successfully.
    Done,
    /// An error occurred during streaming.
    Error(String),
}

/// A pinned boxed stream of LLM events.
pub type LlmStream = Pin<Box<dyn Stream<Item = LlmEvent> + Send>>;

/// Errors from LLM provider operations.
#[derive(Debug, Clone, Error)]
pub enum LlmError {
    /// The provider is unavailable (network error, auth failure, timeout after retries).
    #[error("LLM unavailable: {reason}")]
    Unavailable { reason: String },

    /// The request was malformed (e.g., empty messages).
    #[error("invalid request: {reason}")]
    InvalidRequest { reason: String },

    /// Rate limited by the provider.
    #[error("rate limited")]
    RateLimited,
}

/// Async trait for LLM (Large Language Model) providers.
///
/// Implementations wrap specific LLM APIs (e.g., `DeepSeek`) and provide
/// a unified streaming interface for chat completions.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat request and receive a stream of events.
    async fn chat_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError>;

    /// Return the provider identifier (e.g., "deepseek", "mock").
    fn provider_id(&self) -> &'static str;
}
