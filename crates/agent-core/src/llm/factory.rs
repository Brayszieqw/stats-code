//! LLM provider factory.
//!
//! Selects and constructs an `LlmProvider` based on the unified `LlmConfig`.
//! Single entry point for production code: caller constructs `LlmConfig`,
//! calls `build_llm_provider`, and receives `Arc<dyn LlmProvider>`.

use std::sync::Arc;

use crate::llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use crate::llm::openai::{OpenAiConfig, OpenAiProvider};
use crate::llm::openai_compat::ConfigError;
use crate::traits::llm_provider::LlmProvider;

/// Unified LLM configuration.
///
/// Choose exactly one variant. The factory dispatches to the correct provider.
#[derive(Clone)]
pub enum LlmConfig {
    /// DeepSeek's hosted API (OpenAI-compatible).
    DeepSeek(DeepSeekConfig),
    /// OpenAI's official API (or any OpenAI-compatible endpoint with org support).
    OpenAi(OpenAiConfig),
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmConfig::DeepSeek(c) => f
                .debug_struct("LlmConfig::DeepSeek")
                .field("model", &c.model)
                .field("base_url", &c.base_url)
                .field("api_key", &"[REDACTED]")
                .finish(),
            LlmConfig::OpenAi(c) => f
                .debug_struct("LlmConfig::OpenAi")
                .field("model", &c.model)
                .field("base_url", &c.base_url)
                .field("organization", &c.organization)
                .field("api_key", &"[REDACTED]")
                .finish(),
        }
    }
}

impl LlmConfig {
    /// Returns the provider id ("deepseek" or "openai") for logging.
    #[must_use]
    pub fn provider_id(&self) -> &'static str {
        match self {
            LlmConfig::DeepSeek(_) => DeepSeekProvider::PROVIDER_ID,
            LlmConfig::OpenAi(_) => OpenAiProvider::PROVIDER_ID,
        }
    }

    /// Returns the configured model name.
    #[must_use]
    pub fn model(&self) -> &str {
        match self {
            LlmConfig::DeepSeek(c) => &c.model,
            LlmConfig::OpenAi(c) => &c.model,
        }
    }
}

/// Build an `LlmProvider` from the unified config.
///
/// Validates configuration eagerly; returns `ConfigError` for missing/invalid
/// fields. Callers (e.g. `agent-server::main`) should treat this as a fatal
/// startup error and abort (R13.6).
pub fn build_llm_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, ConfigError> {
    match config {
        LlmConfig::DeepSeek(c) => {
            let provider = DeepSeekProvider::from_config(c)?;
            Ok(Arc::new(provider))
        }
        LlmConfig::OpenAi(c) => {
            let provider = OpenAiProvider::from_config(c)?;
            Ok(Arc::new(provider))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    #[test]
    fn factory_builds_deepseek_provider() {
        let config = LlmConfig::DeepSeek(DeepSeekConfig::new(SecretString::from(
            "sk-test".to_string(),
        )));
        let provider = build_llm_provider(&config).unwrap();
        assert_eq!(provider.provider_id(), "deepseek");
    }

    #[test]
    fn factory_builds_openai_provider() {
        let config = LlmConfig::OpenAi(OpenAiConfig::new(SecretString::from(
            "sk-test".to_string(),
        )));
        let provider = build_llm_provider(&config).unwrap();
        assert_eq!(provider.provider_id(), "openai");
    }

    #[test]
    fn factory_rejects_empty_api_key_deepseek() {
        let config = LlmConfig::DeepSeek(DeepSeekConfig::new(SecretString::from(String::new())));
        assert!(build_llm_provider(&config).is_err());
    }

    #[test]
    fn factory_rejects_empty_api_key_openai() {
        let config = LlmConfig::OpenAi(OpenAiConfig::new(SecretString::from(String::new())));
        assert!(build_llm_provider(&config).is_err());
    }

    #[test]
    fn provider_id_reflects_variant() {
        let ds = LlmConfig::DeepSeek(DeepSeekConfig::new(SecretString::from("k".to_string())));
        let oa = LlmConfig::OpenAi(OpenAiConfig::new(SecretString::from("k".to_string())));
        assert_eq!(ds.provider_id(), "deepseek");
        assert_eq!(oa.provider_id(), "openai");
    }

    #[test]
    fn model_returns_configured_value() {
        let ds = LlmConfig::DeepSeek(DeepSeekConfig::new(SecretString::from("k".to_string())));
        assert_eq!(ds.model(), DeepSeekConfig::DEFAULT_MODEL);

        let oa = LlmConfig::OpenAi(
            OpenAiConfig::new(SecretString::from("k".to_string())).with_model("gpt-4o"),
        );
        assert_eq!(oa.model(), "gpt-4o");
    }
}
