//! Shared LLM configuration contract used across launcher and HTTP handlers.

use serde::{Deserialize, Serialize};

/// LLM provider serialized as the public lowercase API/config value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// DeepSeek service (`provider = "deepseek"`).
    DeepSeek,
    /// OpenAI-compatible service (`provider = "openai"`).
    OpenAi,
}

/// Persisted LLM configuration shape.
///
/// The API key is intentionally plain text at the storage boundary; callers must
/// avoid serializing this type into status responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Selected service provider.
    pub provider: LlmProvider,
    /// Plain API key. Empty means unconfigured.
    pub api_key: String,
    /// Optional API base URL override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Optional model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl LlmConfig {
    /// Whether this record contains a non-empty API key.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_serializes_as_lowercase_api_value() {
        assert_eq!(
            serde_json::to_string(&LlmProvider::DeepSeek).expect("serialize"),
            r#""deepseek""#
        );
        assert_eq!(
            serde_json::to_string(&LlmProvider::OpenAi).expect("serialize"),
            r#""openai""#
        );
    }

    #[test]
    fn empty_api_key_is_unconfigured() {
        let cfg = LlmConfig {
            provider: LlmProvider::DeepSeek,
            api_key: String::new(),
            base_url: None,
            model: None,
        };
        assert!(!cfg.is_configured());
    }
}
