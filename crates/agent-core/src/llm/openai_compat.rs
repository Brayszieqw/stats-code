//! OpenAI-compatible LLM provider implementation.
//!
//! Implements the shared protocol used by both `OpenAI`'s official API and
//! OpenAI-compatible providers (`DeepSeek`, Together, Groq, Ollama, etc.).
//!
//! Endpoint: `POST {base_url}/chat/completions`
//! Request: `{model, messages, stream, max_tokens?, temperature?}`
//! Response: SSE stream of `data: {choices:[{delta:{content}}], usage?:{...}}` frames,
//!           terminated by `data: [DONE]`.
//!
//! Concrete adapters (`DeepSeekProvider`, `OpenAiProvider`) wrap this with
//! provider-specific defaults and `provider_id()`.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;
use url::Url;

use crate::traits::llm_provider::{LlmError, LlmEvent, LlmProvider, LlmRequest, LlmStream};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for an OpenAI-compatible LLM provider.
///
/// Used by both `DeepSeekProvider` (`DeepSeek`'s hosted API) and
/// `OpenAiProvider` (`OpenAI`'s official API) — the wire protocol is identical.
#[derive(Clone)]
pub struct OpenAiCompatConfig {
    /// API key (secret, never logged).
    pub api_key: SecretString,
    /// Base URL for the API (e.g. `https://api.deepseek.com/v1`,
    /// `https://api.openai.com/v1`).
    pub base_url: String,
    /// Model identifier (e.g. `deepseek-chat`, `gpt-4o-mini`).
    pub model: String,
    /// Request timeout in seconds (default 30).
    pub request_timeout_secs: u64,
    /// Maximum retries for 5xx/network errors (default 2).
    pub max_retries: u32,
    /// Optional organization ID (OpenAI-only header `OpenAI-Organization`).
    pub organization: Option<String>,
}

impl OpenAiCompatConfig {
    /// Builder-style constructor with sensible defaults.
    pub fn new(api_key: SecretString, base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.into(),
            model: model.into(),
            request_timeout_secs: 30,
            max_retries: 2,
            organization: None,
        }
    }
}

/// Error returned when configuration validation fails.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("api_key must not be empty")]
    EmptyApiKey,
    #[error("base_url is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("model must not be empty")]
    EmptyModel,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// OpenAI-compatible LLM provider.
///
/// `api_key` is wrapped in `SecretString` — it is never exposed via `Debug` or `Display`.
/// The `provider_id` field carries the upstream brand ("deepseek" / "openai") so
/// the same struct can serve both adapters.
pub struct OpenAiCompatProvider {
    api_key: SecretString,
    base_url: Url,
    model: String,
    http: reqwest::Client,
    max_retries: u32,
    organization: Option<String>,
    provider_id: &'static str,
}

impl std::fmt::Debug for OpenAiCompatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompatProvider")
            .field("provider_id", &self.provider_id)
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url.as_str())
            .field("model", &self.model)
            .field("max_retries", &self.max_retries)
            .field("organization", &self.organization)
            .finish()
    }
}

impl std::fmt::Display for OpenAiCompatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}(model={}, base_url={})",
            self.provider_id,
            self.model,
            self.base_url.as_str()
        )
    }
}

impl OpenAiCompatProvider {
    /// Create a new provider from configuration with the given `provider_id` tag.
    ///
    /// Validates that `api_key`, `base_url`, and `model` are non-empty and well-formed.
    /// Returns `ConfigError` on validation failure (caller should abort startup per R13.6).
    pub fn from_config(
        config: &OpenAiCompatConfig,
        provider_id: &'static str,
    ) -> Result<Self, ConfigError> {
        if config.api_key.expose_secret().is_empty() {
            return Err(ConfigError::EmptyApiKey);
        }

        let base_url = Url::parse(&config.base_url)
            .map_err(|e| ConfigError::InvalidBaseUrl(e.to_string()))?;

        if config.model.trim().is_empty() {
            return Err(ConfigError::EmptyModel);
        }

        let timeout = Duration::from_secs(config.request_timeout_secs.max(1));
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build reqwest client");

        Ok(Self {
            api_key: config.api_key.clone(),
            base_url,
            model: config.model.clone(),
            http,
            max_retries: config.max_retries,
            organization: config.organization.clone(),
            provider_id,
        })
    }

    /// Returns the configured model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the configured retry budget.
    #[must_use]
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Returns the base URL.
    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

// ---------------------------------------------------------------------------
// Retry logic
// ---------------------------------------------------------------------------

/// Categorize an HTTP response or network error for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Success — no retry needed.
    Success,
    /// Retryable failure (5xx, network error, timeout).
    Retryable,
    /// Non-retryable failure (4xx).
    NonRetryable,
}

/// Determine retry decision from an HTTP status code or network error.
#[must_use]
pub fn classify_response(result: &Result<StatusCode, &reqwest::Error>) -> RetryDecision {
    match result {
        Ok(status) if status.is_success() => RetryDecision::Success,
        Ok(status) if status.is_client_error() => RetryDecision::NonRetryable,
        Ok(_) => RetryDecision::Retryable,    // 5xx
        Err(_) => RetryDecision::Retryable,   // network error / timeout
    }
}

/// Compute backoff duration for retry attempt `attempt` (0-indexed).
/// attempt 0 → 1s, attempt 1 → 2s.
#[must_use]
pub fn backoff_duration(attempt: u32) -> Duration {
    Duration::from_secs(1 << attempt) // 1s, 2s
}

// ---------------------------------------------------------------------------
// Wire types (shared OpenAI schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    mode: &'static str,
}

// ---------------------------------------------------------------------------
// LlmProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        let start = std::time::Instant::now();

        let endpoint = self
            .base_url
            .join("chat/completions")
            .map_err(|e| LlmError::InvalidRequest {
                reason: format!("invalid base_url: {e}"),
            })?;

        let body = ChatCompletionRequest {
            model: request_model(&req.model, &self.model),
            messages: req
                .messages
                .iter()
                .map(|m| ChatMessage {
                    role: match m.role {
                        crate::traits::llm_provider::LlmRole::System => "system".to_string(),
                        crate::traits::llm_provider::LlmRole::User => "user".to_string(),
                        crate::traits::llm_provider::LlmRole::Assistant => "assistant".to_string(),
                    },
                    content: m.content.clone(),
                })
                .collect(),
            stream: true,
            max_tokens: request_max_tokens(req.max_tokens, self.provider_id),
            temperature: request_temperature(req.temperature, self.provider_id),
            thinking: request_thinking(self.provider_id),
            reasoning_effort: request_reasoning_effort(self.provider_id),
        };

        let mut attempts = 0u32;
        let max_attempts = self.max_retries + 1;

        let response = loop {
            attempts += 1;

            let mut request_builder = self
                .http
                .post(endpoint.as_str())
                .header(
                    "Authorization",
                    format!("Bearer {}", self.api_key.expose_secret()),
                )
                .header("Content-Type", "application/json");

            if let Some(org) = &self.organization {
                request_builder = request_builder.header("OpenAI-Organization", org);
            }

            let result: Result<reqwest::Response, reqwest::Error> =
                request_builder.json(&body).send().await;

            match &result {
                Ok(resp) => {
                    let status = resp.status();
                    let decision = classify_response(&Ok(status));

                    match decision {
                        RetryDecision::Success => break result.unwrap(),
                        RetryDecision::NonRetryable => {
                            let reason = format!(
                                "{} API returned client error: {} ({})",
                                self.provider_id,
                                status.as_u16(),
                                status.canonical_reason().unwrap_or("unknown")
                            );
                            tracing::warn!(
                                provider = self.provider_id,
                                model = %self.model,
                                status = status.as_u16(),
                                attempts,
                                duration_ms = start.elapsed().as_millis() as u64,
                                "LLM call failed (non-retryable)"
                            );
                            return Err(LlmError::Unavailable { reason });
                        }
                        RetryDecision::Retryable => {
                            if attempts >= max_attempts {
                                let reason = format!(
                                    "{} API returned server error after {} attempts: {}",
                                    self.provider_id,
                                    attempts,
                                    status.as_u16()
                                );
                                tracing::warn!(
                                    provider = self.provider_id,
                                    model = %self.model,
                                    status = status.as_u16(),
                                    attempts,
                                    duration_ms = start.elapsed().as_millis() as u64,
                                    "LLM call failed (retries exhausted)"
                                );
                                return Err(LlmError::Unavailable { reason });
                            }
                            let delay = backoff_duration(attempts - 1);
                            tracing::info!(
                                provider = self.provider_id,
                                model = %self.model,
                                attempt = attempts,
                                delay_ms = delay.as_millis() as u64,
                                "Retrying after server error"
                            );
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
                Err(e) => {
                    let decision = classify_response(&Err(e));
                    debug_assert_eq!(decision, RetryDecision::Retryable);

                    if attempts >= max_attempts {
                        let reason = format!(
                            "{} API network error after {} attempts: {}",
                            self.provider_id, attempts, e
                        );
                        tracing::warn!(
                            provider = self.provider_id,
                            model = %self.model,
                            attempts,
                            duration_ms = start.elapsed().as_millis() as u64,
                            "LLM call failed (network error, retries exhausted)"
                        );
                        return Err(LlmError::Unavailable { reason });
                    }
                    let delay = backoff_duration(attempts - 1);
                    tracing::info!(
                        provider = self.provider_id,
                        model = %self.model,
                        attempt = attempts,
                        delay_ms = delay.as_millis() as u64,
                        "Retrying after network error"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        };

        let model = self.model.clone();
        let provider_id = self.provider_id;
        let byte_stream = response.bytes_stream();
        let event_stream = SseEventStream::new(byte_stream, model, provider_id, start);

        Ok(Box::pin(event_stream))
    }

    fn provider_id(&self) -> &'static str {
        self.provider_id
    }
}

fn request_model(request_model: &str, configured_model: &str) -> String {
    if request_model.trim().is_empty() {
        configured_model.to_string()
    } else {
        request_model.to_string()
    }
}

const DEEPSEEK_MAX_OUTPUT_TOKENS: u32 = 384 * 1024;

fn request_max_tokens(request_max_tokens: Option<u32>, provider_id: &str) -> Option<u32> {
    if provider_id == "deepseek" {
        Some(DEEPSEEK_MAX_OUTPUT_TOKENS)
    } else {
        request_max_tokens
    }
}

fn request_temperature(request_temperature: Option<f32>, provider_id: &str) -> Option<f32> {
    if provider_id == "deepseek" {
        None
    } else {
        request_temperature
    }
}

fn request_thinking(provider_id: &str) -> Option<ThinkingConfig> {
    (provider_id == "deepseek").then_some(ThinkingConfig { mode: "enabled" })
}

fn request_reasoning_effort(provider_id: &str) -> Option<&'static str> {
    (provider_id == "deepseek").then_some("max")
}

// ---------------------------------------------------------------------------
// SSE event stream adapter
// ---------------------------------------------------------------------------

/// Adapter that converts a raw byte stream into `LlmEvent`s by parsing SSE frames.
pub(crate) struct SseEventStream<S> {
    inner: S,
    buffer: String,
    model: String,
    provider_id: &'static str,
    start: std::time::Instant,
    prompt_tokens: u32,
    completion_tokens: u32,
    done: bool,
}

impl<S> SseEventStream<S> {
    pub(crate) fn new(
        inner: S,
        model: String,
        provider_id: &'static str,
        start: std::time::Instant,
    ) -> Self {
        Self {
            inner,
            buffer: String::new(),
            model,
            provider_id,
            start,
            prompt_tokens: 0,
            completion_tokens: 0,
            done: false,
        }
    }
}

impl<S> Stream for SseEventStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin + Send,
{
    type Item = LlmEvent;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        let this = self.get_mut();

        if this.done {
            return Poll::Ready(None);
        }

        if let Some(event) = this.try_parse_next_event() {
            return Poll::Ready(Some(event));
        }

        loop {
            let inner = std::pin::Pin::new(&mut this.inner);
            match inner.poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    let text = String::from_utf8_lossy(&chunk);
                    this.buffer.push_str(&text);

                    if let Some(event) = this.try_parse_next_event() {
                        return Poll::Ready(Some(event));
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    this.done = true;
                    return Poll::Ready(Some(LlmEvent::Error(format!("stream error: {e}"))));
                }
                Poll::Ready(None) => {
                    if !this.done {
                        this.done = true;
                        tracing::info!(
                            provider = this.provider_id,
                            model = %this.model,
                            prompt_tokens = this.prompt_tokens,
                            completion_tokens = this.completion_tokens,
                            duration_ms = this.start.elapsed().as_millis() as u64,
                            "LLM stream completed"
                        );
                        return Poll::Ready(Some(LlmEvent::Done));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<S> SseEventStream<S> {
    fn try_parse_next_event(&mut self) -> Option<LlmEvent> {
        loop {
            let separator_pos = self
                .buffer
                .find("\n\n")
                .map(|p| (p, 2))
                .or_else(|| self.buffer.find("\r\n\r\n").map(|p| (p, 4)));

            let (pos, sep_len) = separator_pos?;
            let frame: String = self.buffer.drain(..pos + sep_len).collect();

            if let Some(event) = self.parse_sse_frame(&frame) {
                return Some(event);
            }
        }
    }

    fn parse_sse_frame(&mut self, frame: &str) -> Option<LlmEvent> {
        let mut data_lines: Vec<&str> = Vec::new();

        for line in frame.lines() {
            if line.starts_with(':') {
                continue;
            }
            if let Some(data) = line.strip_prefix("data:") {
                data_lines.push(data.trim_start());
            }
        }

        if data_lines.is_empty() {
            return None;
        }

        let payload = data_lines.join("\n");

        if payload.trim() == "[DONE]" {
            self.done = true;
            tracing::info!(
                provider = self.provider_id,
                model = %self.model,
                prompt_tokens = self.prompt_tokens,
                completion_tokens = self.completion_tokens,
                duration_ms = self.start.elapsed().as_millis() as u64,
                "LLM stream completed"
            );
            return Some(LlmEvent::Done);
        }

        match serde_json::from_str::<ChatCompletionChunk>(&payload) {
            Ok(chunk) => {
                if let Some(usage) = &chunk.usage {
                    self.prompt_tokens = usage.prompt_tokens;
                    self.completion_tokens = usage.completion_tokens;
                }

                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        if !content.is_empty() {
                            return Some(LlmEvent::TextDelta(content.clone()));
                        }
                    }
                    if choice.finish_reason.is_some() {
                        return None;
                    }
                }
                None
            }
            Err(e) => {
                tracing::debug!(
                    provider = self.provider_id,
                    error = %e,
                    "Failed to parse SSE chunk (may be non-JSON event)"
                );
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone config validation (for Property 28)
// ---------------------------------------------------------------------------

/// Validate an OpenAI-compatible configuration for production readiness.
///
/// Returns `Ok(())` iff:
/// 1. `provider` matches the expected provider id (e.g., "deepseek" or "openai")
/// 2. `api_key` is non-empty
/// 3. `base_url` is a valid URL
/// 4. `model` is non-empty
pub fn validate_config(
    expected_provider: &str,
    actual_provider: &str,
    config: &OpenAiCompatConfig,
) -> Result<(), ConfigError> {
    if expected_provider != actual_provider {
        return Err(ConfigError::InvalidBaseUrl(format!(
            "expected provider '{expected_provider}', got '{actual_provider}'"
        )));
    }
    if config.api_key.expose_secret().is_empty() {
        return Err(ConfigError::EmptyApiKey);
    }
    Url::parse(&config.base_url).map_err(|e| ConfigError::InvalidBaseUrl(e.to_string()))?;
    if config.model.trim().is_empty() {
        return Err(ConfigError::EmptyModel);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (shared infrastructure)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(api_key: &str, base_url: &str, model: &str) -> OpenAiCompatConfig {
        OpenAiCompatConfig::new(
            SecretString::from(api_key.to_string()),
            base_url.to_string(),
            model.to_string(),
        )
    }

    #[test]
    fn from_config_valid() {
        let config = make_config("sk-test", "https://api.example.com/v1/", "model-x");
        let provider = OpenAiCompatProvider::from_config(&config, "test");
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.model, "model-x");
        assert_eq!(p.provider_id, "test");
        assert_eq!(p.max_retries, 2);
    }

    #[test]
    fn from_config_empty_api_key() {
        let config = make_config("", "https://api.example.com/v1/", "model");
        let err = OpenAiCompatProvider::from_config(&config, "test").unwrap_err();
        assert!(matches!(err, ConfigError::EmptyApiKey));
    }

    #[test]
    fn from_config_invalid_url() {
        let config = make_config("sk-key", "not a url", "model");
        let err = OpenAiCompatProvider::from_config(&config, "test").unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBaseUrl(_)));
    }

    #[test]
    fn from_config_empty_model() {
        let config = make_config("sk-key", "https://api.example.com/v1/", "");
        let err = OpenAiCompatProvider::from_config(&config, "test").unwrap_err();
        assert!(matches!(err, ConfigError::EmptyModel));
    }

    #[test]
    fn from_config_whitespace_model() {
        let config = make_config("sk-key", "https://api.example.com/v1/", "   ");
        let err = OpenAiCompatProvider::from_config(&config, "test").unwrap_err();
        assert!(matches!(err, ConfigError::EmptyModel));
    }

    #[test]
    fn empty_request_model_falls_back_to_configured_model() {
        assert_eq!(request_model("", "deepseek-chat"), "deepseek-chat");
        assert_eq!(request_model("   ", "deepseek-chat"), "deepseek-chat");
        assert_eq!(request_model("custom-model", "deepseek-chat"), "custom-model");
    }

    #[test]
    fn deepseek_request_options_are_maxed() {
        assert_eq!(
            request_max_tokens(Some(32), "deepseek"),
            Some(DEEPSEEK_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(request_temperature(Some(0.7), "deepseek"), None);
        assert_eq!(request_reasoning_effort("deepseek"), Some("max"));

        let thinking = request_thinking("deepseek").expect("deepseek thinking enabled");
        let thinking = serde_json::to_value(thinking).unwrap();
        assert_eq!(thinking, serde_json::json!({ "type": "enabled" }));
    }

    #[test]
    fn non_deepseek_request_options_are_preserved() {
        assert_eq!(request_max_tokens(Some(32), "openai"), Some(32));
        assert_eq!(request_max_tokens(None, "openai"), None);
        assert_eq!(request_temperature(Some(0.7), "openai"), Some(0.7));
        assert!(request_thinking("openai").is_none());
        assert_eq!(request_reasoning_effort("openai"), None);
    }

    #[test]
    fn classify_success() {
        assert_eq!(
            classify_response(&Ok(StatusCode::OK)),
            RetryDecision::Success
        );
    }

    #[test]
    fn classify_client_error_non_retryable() {
        assert_eq!(
            classify_response(&Ok(StatusCode::BAD_REQUEST)),
            RetryDecision::NonRetryable
        );
        assert_eq!(
            classify_response(&Ok(StatusCode::UNAUTHORIZED)),
            RetryDecision::NonRetryable
        );
        assert_eq!(
            classify_response(&Ok(StatusCode::FORBIDDEN)),
            RetryDecision::NonRetryable
        );
    }

    #[test]
    fn classify_server_error_retryable() {
        assert_eq!(
            classify_response(&Ok(StatusCode::INTERNAL_SERVER_ERROR)),
            RetryDecision::Retryable
        );
        assert_eq!(
            classify_response(&Ok(StatusCode::BAD_GATEWAY)),
            RetryDecision::Retryable
        );
        assert_eq!(
            classify_response(&Ok(StatusCode::SERVICE_UNAVAILABLE)),
            RetryDecision::Retryable
        );
    }

    #[test]
    fn backoff_durations() {
        assert_eq!(backoff_duration(0), Duration::from_secs(1));
        assert_eq!(backoff_duration(1), Duration::from_secs(2));
        assert_eq!(backoff_duration(2), Duration::from_secs(4));
    }

    #[test]
    fn validate_config_matches_provider() {
        let config = make_config("sk-key", "https://api.example.com/v1/", "model");
        assert!(validate_config("deepseek", "deepseek", &config).is_ok());
        assert!(validate_config("openai", "openai", &config).is_ok());
        assert!(validate_config("openai", "deepseek", &config).is_err());
    }

    #[test]
    fn validate_config_rejects_invalid() {
        let bad_key = make_config("", "https://api.example.com/v1/", "model");
        assert!(validate_config("openai", "openai", &bad_key).is_err());

        let bad_url = make_config("k", "not-a-url", "model");
        assert!(validate_config("openai", "openai", &bad_url).is_err());

        let bad_model = make_config("k", "https://api.example.com/v1/", "");
        assert!(validate_config("openai", "openai", &bad_model).is_err());
    }

    #[test]
    fn debug_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-12345",
            "https://api.example.com/v1/",
            "model",
        );
        let provider = OpenAiCompatProvider::from_config(&config, "test").unwrap();
        let debug_output = format!("{provider:?}");
        assert!(!debug_output.contains("sk-super-secret-key-12345"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn display_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-12345",
            "https://api.example.com/v1/",
            "model",
        );
        let provider = OpenAiCompatProvider::from_config(&config, "test").unwrap();
        let display_output = format!("{provider}");
        assert!(!display_output.contains("sk-super-secret-key-12345"));
    }

    fn make_test_stream(
        buffer: &str,
        provider_id: &'static str,
    ) -> SseEventStream<tokio_stream::Empty<Result<Bytes, reqwest::Error>>> {
        SseEventStream {
            inner: tokio_stream::empty(),
            buffer: buffer.to_string(),
            model: "test".to_string(),
            provider_id,
            start: std::time::Instant::now(),
            prompt_tokens: 0,
            completion_tokens: 0,
            done: false,
        }
    }

    #[test]
    fn parse_sse_text_delta() {
        let frame = "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n";
        let mut stream = make_test_stream(frame, "test");
        let event = stream.try_parse_next_event();
        assert!(matches!(event, Some(LlmEvent::TextDelta(ref s)) if s == "Hello"));
    }

    #[test]
    fn parse_sse_done() {
        let frame = "data: [DONE]\n\n";
        let mut stream = make_test_stream(frame, "test");
        let event = stream.try_parse_next_event();
        assert!(matches!(event, Some(LlmEvent::Done)));
        assert!(stream.done);
    }

    #[test]
    fn parse_sse_ignores_comments() {
        let frame = ": this is a comment\n\ndata: {\"id\":\"x\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n";
        let mut stream = make_test_stream(frame, "test");
        let event = stream.try_parse_next_event();
        assert!(matches!(event, Some(LlmEvent::TextDelta(ref s)) if s == "Hi"));
    }

    #[test]
    fn parse_sse_usage_tracking() {
        let frame = "data: {\"id\":\"x\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20}}\n\n";
        let mut stream = make_test_stream(frame, "test");
        let _ = stream.try_parse_next_event();
        assert_eq!(stream.prompt_tokens, 10);
        assert_eq!(stream.completion_tokens, 20);
    }
}
