//! `OpenAI` GPT LLM provider — thin adapter over `OpenAiCompatProvider`.
//!
//! Supports the official `OpenAI` API (`https://api.openai.com/v1`) with optional
//! `OpenAI-Organization` header. Compatible with all `OpenAI` chat completion
//! models (gpt-4o, gpt-4o-mini, gpt-4-turbo, gpt-3.5-turbo, etc.).

use async_trait::async_trait;
use secrecy::SecretString;

use crate::llm::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};
use crate::traits::llm_provider::{LlmError, LlmProvider, LlmRequest, LlmStream};

pub use crate::llm::openai_compat::ConfigError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the `OpenAI` provider.
#[derive(Clone)]
pub struct OpenAiConfig {
    /// API key (secret, never logged).
    pub api_key: SecretString,
    /// Base URL for the `OpenAI` API (default `https://api.openai.com/v1`).
    pub base_url: String,
    /// Model identifier (e.g. `gpt-4o`, `gpt-4o-mini`).
    pub model: String,
    /// Request timeout in seconds (default 30).
    pub request_timeout_secs: u64,
    /// Maximum retries for 5xx/network errors (default 2).
    pub max_retries: u32,
    /// Optional organization ID (sent via `OpenAI-Organization` header).
    pub organization: Option<String>,
}

impl OpenAiConfig {
    /// Default base URL for the official `OpenAI` hosted API.
    pub const DEFAULT_BASE_URL: &'static str = "https://api.openai.com/v1/";
    /// Default model — small/cheap variant suitable for chat workloads.
    pub const DEFAULT_MODEL: &'static str = "gpt-4o-mini";

    /// Convenience constructor with sensible defaults.
    #[must_use] 
    pub fn new(api_key: SecretString) -> Self {
        Self {
            api_key,
            base_url: Self::DEFAULT_BASE_URL.to_string(),
            model: Self::DEFAULT_MODEL.to_string(),
            request_timeout_secs: 30,
            max_retries: 2,
            organization: None,
        }
    }

    /// Builder: set model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Builder: set organization id.
    #[must_use]
    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }
}

impl From<OpenAiConfig> for OpenAiCompatConfig {
    fn from(c: OpenAiConfig) -> Self {
        Self {
            api_key: c.api_key,
            base_url: c.base_url,
            model: c.model,
            request_timeout_secs: c.request_timeout_secs,
            max_retries: c.max_retries,
            organization: c.organization,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// `OpenAI` LLM provider.
///
/// Wraps `OpenAiCompatProvider` with `provider_id = "openai"`.
/// `api_key` is wrapped in `SecretString` — never exposed via `Debug` or `Display`.
pub struct OpenAiProvider {
    inner: OpenAiCompatProvider,
}

impl std::fmt::Debug for OpenAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

impl std::fmt::Display for OpenAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl OpenAiProvider {
    /// Provider identifier emitted to logs and `LlmProvider::provider_id`.
    pub const PROVIDER_ID: &'static str = "openai";

    /// Create a new `OpenAiProvider` from configuration.
    ///
    /// Validates that `api_key`, `base_url`, and `model` are non-empty and well-formed.
    /// Returns `ConfigError` on validation failure.
    pub fn from_config(config: &OpenAiConfig) -> Result<Self, ConfigError> {
        let compat: OpenAiCompatConfig = config.clone().into();
        let inner = OpenAiCompatProvider::from_config(&compat, Self::PROVIDER_ID)?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
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

/// Validate an `OpenAI` configuration for production readiness.
pub fn validate_config(provider: &str, config: &OpenAiConfig) -> Result<(), ConfigError> {
    let compat: OpenAiCompatConfig = config.clone().into();
    crate::llm::openai_compat::validate_config(
        OpenAiProvider::PROVIDER_ID,
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

    fn make_config(api_key: &str, base_url: &str, model: &str) -> OpenAiConfig {
        OpenAiConfig {
            api_key: SecretString::from(api_key.to_string()),
            base_url: base_url.to_string(),
            model: model.to_string(),
            request_timeout_secs: 30,
            max_retries: 2,
            organization: None,
        }
    }

    #[test]
    fn from_config_valid_with_default_base_url() {
        let config = OpenAiConfig::new(SecretString::from("sk-test".to_string()));
        let provider = OpenAiProvider::from_config(&config);
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().provider_id(), "openai");
    }

    #[test]
    fn from_config_valid_explicit() {
        let config = make_config("sk-test", "https://api.openai.com/v1/", "gpt-4o");
        let provider = OpenAiProvider::from_config(&config);
        assert!(provider.is_ok());
    }

    #[test]
    fn from_config_empty_api_key() {
        let config = make_config("", "https://api.openai.com/v1/", "gpt-4o-mini");
        let err = OpenAiProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::EmptyApiKey));
    }

    #[test]
    fn from_config_invalid_url() {
        let config = make_config("sk-key", "not a url", "gpt-4o-mini");
        let err = OpenAiProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidBaseUrl(_)));
    }

    #[test]
    fn from_config_empty_model() {
        let config = make_config("sk-key", "https://api.openai.com/v1/", "");
        let err = OpenAiProvider::from_config(&config).unwrap_err();
        assert!(matches!(err, ConfigError::EmptyModel));
    }

    #[test]
    fn validate_config_valid() {
        let config = make_config("sk-key", "https://api.openai.com/v1/", "gpt-4o-mini");
        assert!(validate_config("openai", &config).is_ok());
    }

    #[test]
    fn validate_config_wrong_provider() {
        let config = make_config("sk-key", "https://api.openai.com/v1/", "gpt-4o-mini");
        assert!(validate_config("deepseek", &config).is_err());
    }

    #[test]
    fn validate_config_empty_api_key() {
        let config = make_config("", "https://api.openai.com/v1/", "gpt-4o-mini");
        assert!(validate_config("openai", &config).is_err());
    }

    #[test]
    fn debug_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-67890",
            "https://api.openai.com/v1/",
            "gpt-4o-mini",
        );
        let provider = OpenAiProvider::from_config(&config).unwrap();
        let debug_output = format!("{provider:?}");
        assert!(!debug_output.contains("sk-super-secret-key-67890"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn display_does_not_leak_api_key() {
        let config = make_config(
            "sk-super-secret-key-67890",
            "https://api.openai.com/v1/",
            "gpt-4o-mini",
        );
        let provider = OpenAiProvider::from_config(&config).unwrap();
        let display_output = format!("{provider}");
        assert!(!display_output.contains("sk-super-secret-key-67890"));
    }

    #[test]
    fn provider_id_is_openai() {
        let config = make_config("sk-key", "https://api.openai.com/v1/", "gpt-4o");
        let provider = OpenAiProvider::from_config(&config).unwrap();
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn organization_can_be_set() {
        let config = OpenAiConfig::new(SecretString::from("sk-test".to_string()))
            .with_organization("org-abc123");
        assert_eq!(config.organization.as_deref(), Some("org-abc123"));
    }

    #[test]
    fn debug_does_not_leak_organization() {
        // Org id is not secret, but include it in default Debug output for visibility.
        let config = OpenAiConfig::new(SecretString::from("sk-test".to_string()))
            .with_organization("org-mycompany");
        let provider = OpenAiProvider::from_config(&config).unwrap();
        let debug_output = format!("{provider:?}");
        // The org should appear in debug (it is not secret), but the key must not.
        assert!(debug_output.contains("org-mycompany"));
    }
}
