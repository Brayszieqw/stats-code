//! `SttProvider` trait definition.

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

/// Result of a speech-to-text transcription.
#[derive(Debug, Clone)]
pub struct SttResult {
    /// The transcribed text.
    pub text: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
}

/// Errors from STT provider operations.
#[derive(Debug, Clone, Error)]
pub enum SttError {
    /// The audio format is not supported.
    #[error("unsupported audio format: {reason}")]
    UnsupportedFormat { reason: String },

    /// The provider is unavailable.
    #[error("STT service unavailable: {reason}")]
    Unavailable { reason: String },

    /// Transcription failed.
    #[error("transcription failed: {reason}")]
    TranscriptionFailed { reason: String },
}

/// Async trait for Speech-to-Text providers.
///
/// Implementations wrap specific STT APIs (e.g., `OpenAI` Whisper-compatible)
/// and provide a unified interface for audio transcription.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe audio bytes into text.
    async fn transcribe(&self, audio: Bytes) -> Result<SttResult, SttError>;
}
