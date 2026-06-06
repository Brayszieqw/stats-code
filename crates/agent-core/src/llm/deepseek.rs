//! `DeepSeek` LLM provider — thin adapter over `OpenAiCompatProvider`.
//!
//! `DeepSeek`'s API is OpenAI-compatible (same `chat/completions` endpoint,
//! same SSE schema). This module wraps the shared adapter with DeepSeek-specific
//! defaults and the `provider_id = "deepseek"` tag.

use async_trait::async_trait;
use secrecy::SecretString;

use crate::llm::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};
use crate::traits::llm_provider::{LlmError, LlmProvider, LlmRequest, LlmStream};

pub use crate::llm::openai_compat::ConfigError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the `DeepSeek` provider.
#[derive(Clone)]
pub struct DeepSeekConfig {
    /// API key (secret, never logged).
    pub api_key: SecretString,
    /// Base URL for the `DeepSeek` API (e.g. `https://api.deepseek.com/v1`).
    pub base_url: String,
    /// Model identifier (e.g. `deepseek-chat`).
    pub model: String,
    /// Request timeout in seconds (default 30).
    pub request_timeout_secs: u64,
    /// Maximum retries for 5xx/network errors (default 2).
    pub max_retries: u32,
}

impl DeepSeekConfig {
    /// Default base URL for the `DeepSeek` hosted API.
    pub const DEFAULT_BASE_URL: &'static str = "https://api.deepseek.com/v1/";
    /// Default model.
    pub const DEFAULT_MODEL: &'static str = "deepseek-chat";

    /// Convenience constructor with sensible defaults.
    #[must_use] 
    pub fn new(api_key: SecretString) -> Self {
        Self {
            api_key,
            base_url: Self::DEFAULT_BASE_URL.to_string(),
            model: Self::DEFAULT_MODEL.to_string(),
            request_timeout_secs: 30,
            max_retries: 2,
        }
    }
}

impl From<DeepSeekConfig> for OpenAiCompatConfig {
    fn from(c: DeepSeekConfig) -> Self {
        Self {
            api_key: c.api_key,
            base_url: c.base_url,
            model: c.model,
            request_timeout_secs: c.request_timeout_secs,
            max_retries: c.max_retries,
            organization: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// `DeepSeek` LLM provider.
///
/// Wraps `OpenAiCompatProvider` with `provider_id = "deepseek"`.
/// `api_key` is wrapped in `SecretString` — never exposed via `Debug` or `Display`.
pub struct DeepSeekProvider {
    inner: OpenAiCompatProvider,
}

impl std::fmt::Debug for DeepSeekProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

impl std::fmt::Display for DeepSeekProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl DeepSeekProvider {
    /// Provider identifier emitted to logs and `LlmProvider::provider_id`.
    pub const PROVIDER_ID: &'static str = "deepseek";

    /// Create a new `DeepSeekProvider` from configuration.
    ///
    /// Validates that `api_key`, `base_url`, and `model` are non-empty and well-formed.
    /// Returns `ConfigError` on validation failure (caller should abort startup per R13.6).
    pub fn from_config(config: &DeepSeekConfig) -> Result<Self, ConfigError> {
        let compat: OpenAiCompatConfig = config.clone().into();
        let inner = OpenAiCompatProvider::from_config(&compat, Self::PROVIDER_ID)?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn chat_stream(&self, req: LlmRequest) -> Result<LlmStream, LlmError> {
        self.inner.chat_stream(req).await
    }

    fn provider_id(&self) -> &'static str {
        Self::PROVIDER_ID
    }
}

// ---------------------------------------------------------------------------
// Standalone config validation (for Property 28)
// ---------------------------------------------------------------------------

/// Validate a `DeepSeek` configuration for production readiness.
///
/// Returns `Ok(())` iff:
/// 1. `provider` == "deepseek"
/// 2. `api_key` is non-empty
/// 3. `base_url` is a valid URL
/// 4. `model` is non-empty
pub fn validate_config(provider: &str, config: &DeepSeekConfig) -> Result<(), ConfigError> {
    let compat: OpenAiCompatConfig = config.clone().into();
    crate::llm::openai_compat::validate_config(
        DeepSeekProvider::PROVIDER_ID,
        provider,
        &compat,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(api_key: &str, base_url: &str, model: &str) -> DeepSeekConfig {
        DeepSeekConfig {
            api_key: SecretString::from(api_key.to_string()),
            base_url: base_url.to_string(),
            model: model.to_string(),
            request_timeout_secs: 30,
            max_retries: 2,
        }
    }

    #[test]
    fn from_config_valid() {
        let config = make_config("sk-test-key", "https://api.deepseek.com/v1/", "deepseek-chat");
        let provider = DeepSeekProvider::from_config(&config);
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().provider_id(), "deepseek");
    }

    #[test]
    fn from_config_empty_api_key() {
        let config = make_config("", "https://api.deepseek.com/v1/", "deepseek-chat");
        let err = DeepSeekProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::EmptyApiKey));
    }

    #[test]
    fn from_config_invalid_url() {
        let config = make_config("sk-key", "not a url", "deepseek-chat");
        let err = DeepSeekProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBaseUrl(_)));
    }

    #[test]
    fn from_config_empty_model() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "");
        let err = DeepSeekProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::EmptyModel));
    }

    #[test]
    fn from_config_whitespace_model() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "   ");
        let err = DeepSeekProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::EmptyModel));
    }

    #[test]
    fn validate_config_valid() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "deepseek-chat");
        assert!(validate_config("deepseek", &config).is_ok());
    }

    #[test]
    fn validate_config_wrong_provider() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "deepseek-chat");
        assert!(validate_config("openai", &config).is_err());
    }

    #[test]
    fn validate_config_empty_api_key() {
        let config = make_config("", "https://api.deepseek.com/v1/", "deepseek-chat");
        assert!(validate_config("deepseek", &config).is_err());
    }

    #[test]
    fn validate_config_invalid_url() {
        let config = make_config("sk-key", "://bad", "deepseek-chat");
        assert!(validate_config("deepseek", &config).is_err());
    }

    #[test]
    fn validate_config_empty_model() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "");
        assert!(validate_config("deepseek", &config).is_err());
    }

    #[test]
    fn debug_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-12345",
            "https://api.deepseek.com/v1/",
            "deepseek-chat",
        );
        let provider = DeepSeekProvider::from_config(&config).unwrap();
        let debug_output = format!("{provider:?}");
        assert!(!debug_output.contains("sk-super-secret-key-12345"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn display_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-12345",
            "https://api.deepseek.com/v1/",
            "deepseek-chat",
        );
        let provider = DeepSeekProvider::from_config(&config).unwrap();
        let display_output = format!("{provider}");
        assert!(!display_output.contains("sk-super-secret-key-12345"));
    }

    #[test]
    fn provider_id_is_deepseek() {
        let config = make_config("sk-key", "https://api.deepseek.com/v1/", "deepseek-chat");
        let provider = DeepSeekProvider::from_config(&config).unwrap();
        assert_eq!(provider.provider_id(), "deepseek");
    }
}
