use std::future::Future;
use std::pin::Pin;

use crate::error::ApiError;
use crate::types::{MessageRequest, MessageResponse};

pub mod anthropic_provider;
pub mod openai_compat;

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;

pub trait Provider {
    type Stream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse>;

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Gemini,
    DeepSeek,
    DashScope,
    Moonshot,
    Xai,
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

const fn anthropic_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::Anthropic,
        auth_env: "ANTHROPIC_API_KEY",
        base_url_env: "ANTHROPIC_BASE_URL",
        default_base_url: anthropic_provider::DEFAULT_BASE_URL,
    }
}

const fn openai_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::OpenAi,
        auth_env: "OPENAI_API_KEY",
        base_url_env: "OPENAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
    }
}

const fn gemini_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::Gemini,
        auth_env: "GEMINI_API_KEY",
        base_url_env: "GEMINI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_GEMINI_BASE_URL,
    }
}

const fn deepseek_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::DeepSeek,
        auth_env: "DEEPSEEK_API_KEY",
        base_url_env: "DEEPSEEK_BASE_URL",
        default_base_url: openai_compat::DEFAULT_DEEPSEEK_BASE_URL,
    }
}

const fn dashscope_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::DashScope,
        auth_env: "DASHSCOPE_API_KEY",
        base_url_env: "DASHSCOPE_BASE_URL",
        default_base_url: openai_compat::DEFAULT_DASHSCOPE_BASE_URL,
    }
}

const fn moonshot_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::Moonshot,
        auth_env: "MOONSHOT_API_KEY",
        base_url_env: "MOONSHOT_BASE_URL",
        default_base_url: openai_compat::DEFAULT_MOONSHOT_BASE_URL,
    }
}

const fn xai_metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: ProviderKind::Xai,
        auth_env: "XAI_API_KEY",
        base_url_env: "XAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
    }
}

const MODEL_REGISTRY: &[(&str, ProviderMetadata)] = &[
    ("openai", openai_metadata()),
    ("gpt", openai_metadata()),
    ("gpt-5.4", openai_metadata()),
    ("gpt-mini", openai_metadata()),
    ("gpt-5.4-mini", openai_metadata()),
    ("gpt-nano", openai_metadata()),
    ("gpt-5.4-nano", openai_metadata()),
    ("gemini", gemini_metadata()),
    ("gemini-pro", gemini_metadata()),
    ("gemini-2.5-pro", gemini_metadata()),
    ("gemini-flash", gemini_metadata()),
    ("gemini-2.5-flash", gemini_metadata()),
    ("deepseek", deepseek_metadata()),
    ("deepseek-chat", deepseek_metadata()),
    ("deepseek-reasoner", deepseek_metadata()),
    ("grok", xai_metadata()),
    ("grok-3", xai_metadata()),
    ("grok-mini", xai_metadata()),
    ("grok-3-mini", xai_metadata()),
    ("grok-2", xai_metadata()),
    ("opus", anthropic_metadata()),
    ("sonnet", anthropic_metadata()),
    ("haiku", anthropic_metadata()),
    ("claude-opus-4-6", anthropic_metadata()),
    ("claude-sonnet-4-6", anthropic_metadata()),
    ("claude-haiku-4-5-20251213", anthropic_metadata()),
];

#[must_use]
pub fn resolve_model_alias(model: &str) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "openai" | "gpt" => "gpt-5.4".to_string(),
        "gpt-mini" => "gpt-5.4-mini".to_string(),
        "gpt-nano" => "gpt-5.4-nano".to_string(),
        "gemini" | "gemini-pro" => "gemini-2.5-pro".to_string(),
        "gemini-flash" => "gemini-2.5-flash".to_string(),
        "deepseek" => "deepseek-chat".to_string(),
        "grok" | "grok-3" => "grok-3".to_string(),
        "grok-mini" | "grok-3-mini" => "grok-3-mini".to_string(),
        "grok-2" => "grok-2".to_string(),
        "opus" => "claude-opus-4-6".to_string(),
        "sonnet" => "claude-sonnet-4-6".to_string(),
        "haiku" => "claude-haiku-4-5-20251213".to_string(),
        _ => trimmed.to_string(),
    }
}

#[must_use]
pub fn metadata_for_model(model: &str) -> Option<ProviderMetadata> {
    let canonical = resolve_model_alias(model);
    let lower = canonical.to_ascii_lowercase();
    if let Some((_, metadata)) = MODEL_REGISTRY.iter().find(|(alias, _)| *alias == lower) {
        return Some(*metadata);
    }
    if lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
    {
        return Some(openai_metadata());
    }
    if lower.starts_with("gemini") {
        return Some(gemini_metadata());
    }
    if lower.starts_with("deepseek") {
        return Some(deepseek_metadata());
    }
    if lower.starts_with("qwen") || lower.starts_with("qwq") || lower.starts_with("qvq") {
        return Some(dashscope_metadata());
    }
    if lower.starts_with("kimi") || lower.starts_with("moonshot") {
        return Some(moonshot_metadata());
    }
    if lower.starts_with("grok") {
        return Some(xai_metadata());
    }
    if lower.starts_with("claude") {
        return Some(anthropic_metadata());
    }
    None
}

#[must_use]
pub fn detect_provider_kind(model: &str) -> ProviderKind {
    if let Some(metadata) = metadata_for_model(model) {
        return metadata.provider;
    }
    if openai_compat::has_api_key("OPENAI_API_KEY") {
        return ProviderKind::OpenAi;
    }
    if openai_compat::has_api_key("GEMINI_API_KEY") {
        return ProviderKind::Gemini;
    }
    if openai_compat::has_api_key("DEEPSEEK_API_KEY") {
        return ProviderKind::DeepSeek;
    }
    if openai_compat::has_api_key("DASHSCOPE_API_KEY") {
        return ProviderKind::DashScope;
    }
    if openai_compat::has_api_key("MOONSHOT_API_KEY") {
        return ProviderKind::Moonshot;
    }
    if openai_compat::has_api_key("XAI_API_KEY") {
        return ProviderKind::Xai;
    }
    if anthropic_provider::has_auth_from_env_or_saved().unwrap_or(false) {
        return ProviderKind::Anthropic;
    }
    ProviderKind::OpenAi
}

#[must_use]
pub fn max_tokens_for_model(model: &str) -> u32 {
    let canonical = resolve_model_alias(model);
    if canonical.contains("opus") {
        32_000
    } else {
        64_000
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_provider_kind, max_tokens_for_model, resolve_model_alias, ProviderKind};

    #[test]
    fn resolves_openai_and_grok_aliases() {
        assert_eq!(resolve_model_alias("openai"), "gpt-5.4");
        assert_eq!(resolve_model_alias("gpt"), "gpt-5.4");
        assert_eq!(resolve_model_alias("gpt-mini"), "gpt-5.4-mini");
        assert_eq!(resolve_model_alias("gemini"), "gemini-2.5-pro");
        assert_eq!(resolve_model_alias("gemini-flash"), "gemini-2.5-flash");
        assert_eq!(resolve_model_alias("deepseek"), "deepseek-chat");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
        assert_eq!(resolve_model_alias("grok-2"), "grok-2");
    }

    #[test]
    fn detects_provider_from_model_name_first() {
        assert_eq!(detect_provider_kind("gpt"), ProviderKind::OpenAi);
        assert_eq!(detect_provider_kind("gemini-2.5-pro"), ProviderKind::Gemini);
        assert_eq!(
            detect_provider_kind("deepseek-chat"),
            ProviderKind::DeepSeek
        );
        assert_eq!(detect_provider_kind("qwen-max"), ProviderKind::DashScope);
        assert_eq!(detect_provider_kind("kimi-k2"), ProviderKind::Moonshot);
        assert_eq!(detect_provider_kind("grok"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::Anthropic
        );
    }

    #[test]
    fn keeps_existing_max_token_heuristic() {
        assert_eq!(max_tokens_for_model("opus"), 32_000);
        assert_eq!(max_tokens_for_model("grok-3"), 64_000);
    }
}
