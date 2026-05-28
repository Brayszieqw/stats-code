//! LLM provider implementations.
//!
//! Two production providers are supplied, both speaking the OpenAI-compatible
//! `chat/completions` SSE protocol via a shared transport in `openai_compat`:
//!
//! - [`DeepSeekProvider`] — `DeepSeek` hosted API (`https://api.deepseek.com/v1`)
//! - [`OpenAiProvider`]   — `OpenAI` official API (`https://api.openai.com/v1`)
//!
//! Use [`LlmConfig`] + [`build_llm_provider`] for unified dispatch.

pub mod deepseek;
pub mod factory;
pub mod mock;
pub mod openai;
pub mod openai_compat;

pub use deepseek::{DeepSeekConfig, DeepSeekProvider};
pub use factory::{build_llm_provider, LlmConfig};
pub use mock::{MockLlm, MockLlmResponse};
pub use openai::{OpenAiConfig, OpenAiProvider};
pub use openai_compat::ConfigError;
