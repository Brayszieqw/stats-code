//! Server configuration: load from YAML file and/or environment variables.
//!
//! Layout (config/agent.yaml):
//! ```yaml
//! server:
//!   bind: "127.0.0.1:8080"
//!   max_concurrent_sessions: 50
//! ai:
//!   provider: "deepseek"        # "deepseek" or "openai"
//!   deepseek:
//!     api_key: "${DEEPSEEK_API_KEY}"   # ${VAR} expanded from env
//!     model: "deepseek-chat"
//!     base_url: "https://api.deepseek.com/v1/"
//!     request_timeout_secs: 30
//!     max_retries: 2
//!   openai:
//!     api_key: "${OPENAI_API_KEY}"
//!     model: "gpt-4o-mini"
//!     base_url: "https://api.openai.com/v1/"
//!     organization: null
//!     request_timeout_secs: 30
//!     max_retries: 2
//! storage:
//!   session_store: "sled:./data/sessions"   # "mem" or "sled:<path>"
//!   dataset_root: "./data/datasets"
//! skill_runner:
//!   stats_code_bin: "stats-code"
//!   max_wall_secs: 60
//!   max_rss_mib: 1024
//! ```

use std::path::PathBuf;

use secrecy::SecretString;
use serde::Deserialize;
use thiserror::Error;

use agent_core::llm::{DeepSeekConfig, LlmConfig, OpenAiConfig};

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub ai: AiConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub skill_runner: SkillRunnerConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_max_concurrent_sessions")]
    pub max_concurrent_sessions: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            max_concurrent_sessions: default_max_concurrent_sessions(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_string()
}
fn default_max_concurrent_sessions() -> u32 {
    50
}

#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    /// "deepseek" or "openai".
    pub provider: String,
    pub deepseek: Option<DeepSeekRawConfig>,
    pub openai: Option<OpenAiRawConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeepSeekRawConfig {
    pub api_key: String,
    #[serde(default = "default_deepseek_model")]
    pub model: String,
    #[serde(default = "default_deepseek_base_url")]
    pub base_url: String,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_deepseek_model() -> String {
    "deepseek-chat".to_string()
}
fn default_deepseek_base_url() -> String {
    "https://api.deepseek.com/v1/".to_string()
}
fn default_request_timeout_secs() -> u64 {
    30
}
fn default_max_retries() -> u32 {
    2
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenAiRawConfig {
    pub api_key: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_openai_base_url() -> String {
    "https://api.openai.com/v1/".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    /// `mem` for in-memory or `sled:<path>` for sled-backed.
    #[serde(default = "default_session_store")]
    pub session_store: String,
    #[serde(default = "default_dataset_root")]
    pub dataset_root: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            session_store: default_session_store(),
            dataset_root: default_dataset_root(),
        }
    }
}

fn default_session_store() -> String {
    "mem".to_string()
}
fn default_dataset_root() -> PathBuf {
    PathBuf::from("./data/datasets")
}

#[derive(Debug, Deserialize, Clone)]
pub struct SkillRunnerConfig {
    #[serde(default = "default_stats_code_bin")]
    pub stats_code_bin: PathBuf,
    #[serde(default = "default_max_wall_secs")]
    pub max_wall_secs: u32,
    #[serde(default = "default_max_rss_mib")]
    pub max_rss_mib: u32,
}

impl Default for SkillRunnerConfig {
    fn default() -> Self {
        Self {
            stats_code_bin: default_stats_code_bin(),
            max_wall_secs: default_max_wall_secs(),
            max_rss_mib: default_max_rss_mib(),
        }
    }
}

fn default_stats_code_bin() -> PathBuf {
    PathBuf::from("stats-code")
}
fn default_max_wall_secs() -> u32 {
    60
}
fn default_max_rss_mib() -> u32 {
    1024
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("AI provider '{0}' is not supported (expected 'deepseek' or 'openai')")]
    UnknownProvider(String),
    #[error("AI section for provider '{0}' is missing")]
    MissingProviderSection(String),
    #[error("LLM provider validation failed: {0}")]
    LlmValidation(#[from] agent_core::llm::ConfigError),
    #[error("api_key is empty (resolved from env or yaml field)")]
    EmptyApiKey,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

impl Config {
    /// Load and parse YAML from a file path, expanding `${VAR}` placeholders
    /// from the process environment.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(ConfigError::FileNotFound(path.to_path_buf()));
        }
        let raw = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&raw)
    }

    /// Parse YAML from a string. `${VAR}` placeholders are expanded from env.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, ConfigError> {
        let expanded = expand_env_vars(yaml);
        let cfg: Config = serde_yaml::from_str(&expanded)?;
        Ok(cfg)
    }

    /// Build the unified [`LlmConfig`] from this config, validating in the process.
    pub fn build_llm_config(&self) -> Result<LlmConfig, ConfigError> {
        match self.ai.provider.as_str() {
            "deepseek" => {
                let raw =
                    self.ai.deepseek.as_ref().ok_or_else(|| {
                        ConfigError::MissingProviderSection("deepseek".to_string())
                    })?;
                if raw.api_key.trim().is_empty() {
                    return Err(ConfigError::EmptyApiKey);
                }
                Ok(LlmConfig::DeepSeek(DeepSeekConfig {
                    api_key: SecretString::from(raw.api_key.clone()),
                    model: raw.model.clone(),
                    base_url: raw.base_url.clone(),
                    request_timeout_secs: raw.request_timeout_secs,
                    max_retries: raw.max_retries,
                }))
            }
            "openai" => {
                let raw = self
                    .ai
                    .openai
                    .as_ref()
                    .ok_or_else(|| ConfigError::MissingProviderSection("openai".to_string()))?;
                if raw.api_key.trim().is_empty() {
                    return Err(ConfigError::EmptyApiKey);
                }
                Ok(LlmConfig::OpenAi(OpenAiConfig {
                    api_key: SecretString::from(raw.api_key.clone()),
                    model: raw.model.clone(),
                    base_url: raw.base_url.clone(),
                    organization: raw.organization.clone(),
                    request_timeout_secs: raw.request_timeout_secs,
                    max_retries: raw.max_retries,
                }))
            }
            other => Err(ConfigError::UnknownProvider(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// Env var expansion
// ---------------------------------------------------------------------------

/// Expand `${VAR}` placeholders in `s` from process environment.
/// Unknown variables are left as-is.
fn expand_env_vars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            let mut found_close = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    found_close = true;
                    break;
                }
                name.push(nc);
            }
            if found_close {
                if let Ok(val) = std::env::var(&name) {
                    out.push_str(&val);
                } else {
                    out.push_str("${");
                    out.push_str(&name);
                    out.push('}');
                }
            } else {
                out.push_str("${");
                out.push_str(&name);
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn parses_deepseek_config() {
        let yaml = r#"
ai:
  provider: deepseek
  deepseek:
    api_key: "sk-test-123"
    model: "deepseek-chat"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(cfg.ai.provider, "deepseek");
        let llm = cfg.build_llm_config().unwrap();
        match llm {
            LlmConfig::DeepSeek(c) => {
                assert_eq!(c.api_key.expose_secret(), "sk-test-123");
                assert_eq!(c.model, "deepseek-chat");
            }
            _ => panic!("expected DeepSeek variant"),
        }
    }

    #[test]
    fn parses_openai_config() {
        let yaml = r#"
ai:
  provider: openai
  openai:
    api_key: "sk-openai-test"
    model: "gpt-4o"
    organization: "org-abc"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(cfg.ai.provider, "openai");
        let llm = cfg.build_llm_config().unwrap();
        match llm {
            LlmConfig::OpenAi(c) => {
                assert_eq!(c.api_key.expose_secret(), "sk-openai-test");
                assert_eq!(c.model, "gpt-4o");
                assert_eq!(c.organization.as_deref(), Some("org-abc"));
            }
            _ => panic!("expected OpenAi variant"),
        }
    }

    #[test]
    fn rejects_unknown_provider() {
        let yaml = r#"
ai:
  provider: anthropic
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        let err = cfg.build_llm_config().unwrap_err();
        assert!(matches!(err, ConfigError::UnknownProvider(p) if p == "anthropic"));
    }

    #[test]
    fn rejects_missing_provider_section() {
        let yaml = r#"
ai:
  provider: openai
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        let err = cfg.build_llm_config().unwrap_err();
        assert!(matches!(err, ConfigError::MissingProviderSection(p) if p == "openai"));
    }

    #[test]
    fn rejects_empty_api_key() {
        let yaml = r#"
ai:
  provider: deepseek
  deepseek:
    api_key: ""
    model: "deepseek-chat"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        let err = cfg.build_llm_config().unwrap_err();
        assert!(matches!(err, ConfigError::EmptyApiKey));
    }

    #[test]
    fn expands_env_vars() {
        std::env::set_var("STATS_CODE_AGENT_SERVER_TEST_API_KEY", "agent-server");

        let yaml = r#"
ai:
  provider: deepseek
  deepseek:
    api_key: "${STATS_CODE_AGENT_SERVER_TEST_API_KEY}"
    model: "deepseek-chat"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        let llm = cfg.build_llm_config().unwrap();
        match llm {
            LlmConfig::DeepSeek(c) => {
                use secrecy::ExposeSecret;
                assert_eq!(c.api_key.expose_secret(), "agent-server");
            }
            _ => panic!("expected DeepSeek"),
        }
    }

    #[test]
    fn unknown_env_var_left_as_is() {
        // Use a name guaranteed not to exist.
        let yaml = r#"
ai:
  provider: deepseek
  deepseek:
    api_key: "${TOTALLY_UNDEFINED_VAR_XYZ_12345_ABCDEF}"
    model: "deepseek-chat"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        let llm = cfg.build_llm_config().unwrap();
        match llm {
            LlmConfig::DeepSeek(c) => {
                use secrecy::ExposeSecret;
                assert_eq!(
                    c.api_key.expose_secret(),
                    "${TOTALLY_UNDEFINED_VAR_XYZ_12345_ABCDEF}"
                );
            }
            _ => panic!("expected DeepSeek"),
        }
    }

    #[test]
    fn defaults_applied_when_missing() {
        let yaml = r#"
ai:
  provider: deepseek
  deepseek:
    api_key: "k"
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(cfg.server.bind, "127.0.0.1:8080");
        assert_eq!(cfg.server.max_concurrent_sessions, 50);
        assert_eq!(cfg.skill_runner.max_wall_secs, 60);
        assert_eq!(cfg.skill_runner.max_rss_mib, 1024);
    }
}
