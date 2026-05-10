use crate::error::ApiError;
use crate::providers::claw_provider::{self, AuthSource, ClawApiClient};
use crate::providers::openai_compat::{self, OpenAiCompatClient, OpenAiCompatConfig};
use crate::providers::{self, Provider, ProviderKind};
use crate::types::{MessageRequest, MessageResponse, StreamEvent};

async fn send_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<MessageResponse, ApiError> {
    provider.send_message(request).await
}

async fn stream_via_provider<P: Provider>(
    provider: &P,
    request: &MessageRequest,
) -> Result<P::Stream, ApiError> {
    provider.stream_message(request).await
}

#[derive(Debug, Clone)]
pub enum ProviderClient {
    OpenAi(OpenAiCompatClient),
    Gemini(OpenAiCompatClient),
    DeepSeek(OpenAiCompatClient),
    DashScope(OpenAiCompatClient),
    Moonshot(OpenAiCompatClient),
    Xai(OpenAiCompatClient),
    ClawApi(ClawApiClient),
}

impl ProviderClient {
    pub fn from_model(model: &str) -> Result<Self, ApiError> {
        Self::from_model_with_default_auth(model, None)
    }

    pub fn from_model_with_default_auth(
        model: &str,
        default_auth: Option<AuthSource>,
    ) -> Result<Self, ApiError> {
        let resolved_model = providers::resolve_model_alias(model);
        match providers::detect_provider_kind(&resolved_model) {
            ProviderKind::OpenAi => Ok(Self::OpenAi(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::openai(),
            )?)),
            ProviderKind::Gemini => Ok(Self::Gemini(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::gemini(),
            )?)),
            ProviderKind::DeepSeek => Ok(Self::DeepSeek(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::deepseek(),
            )?)),
            ProviderKind::DashScope => Ok(Self::DashScope(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::dashscope(),
            )?)),
            ProviderKind::Moonshot => Ok(Self::Moonshot(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::moonshot(),
            )?)),
            ProviderKind::Xai => Ok(Self::Xai(OpenAiCompatClient::from_env(
                OpenAiCompatConfig::xai(),
            )?)),
            ProviderKind::ClawApi => Ok(Self::ClawApi(match default_auth {
                Some(auth) => ClawApiClient::from_auth(auth),
                None => ClawApiClient::from_env()?,
            })),
        }
    }

    #[must_use]
    pub const fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::OpenAi(_) => ProviderKind::OpenAi,
            Self::Gemini(_) => ProviderKind::Gemini,
            Self::DeepSeek(_) => ProviderKind::DeepSeek,
            Self::DashScope(_) => ProviderKind::DashScope,
            Self::Moonshot(_) => ProviderKind::Moonshot,
            Self::Xai(_) => ProviderKind::Xai,
            Self::ClawApi(_) => ProviderKind::ClawApi,
        }
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        match self {
            Self::OpenAi(client)
            | Self::Gemini(client)
            | Self::DeepSeek(client)
            | Self::DashScope(client)
            | Self::Moonshot(client)
            | Self::Xai(client) => send_via_provider(client, request).await,
            Self::ClawApi(client) => send_via_provider(client, request).await,
        }
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        match self {
            Self::OpenAi(client)
            | Self::Gemini(client)
            | Self::DeepSeek(client)
            | Self::DashScope(client)
            | Self::Moonshot(client)
            | Self::Xai(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::OpenAiCompat),
            Self::ClawApi(client) => stream_via_provider(client, request)
                .await
                .map(MessageStream::ClawApi),
        }
    }
}

#[derive(Debug)]
pub enum MessageStream {
    ClawApi(claw_provider::MessageStream),
    OpenAiCompat(openai_compat::MessageStream),
}

impl MessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::ClawApi(stream) => stream.request_id(),
            Self::OpenAiCompat(stream) => stream.request_id(),
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        match self {
            Self::ClawApi(stream) => stream.next_event().await,
            Self::OpenAiCompat(stream) => stream.next_event().await,
        }
    }
}

pub use claw_provider::{
    oauth_token_is_expired, resolve_saved_oauth_token, resolve_startup_auth_source,
};
#[must_use]
pub fn read_base_url() -> String {
    claw_provider::read_base_url()
}

#[must_use]
pub fn read_xai_base_url() -> String {
    openai_compat::read_base_url(OpenAiCompatConfig::xai())
}

#[cfg(test)]
mod tests {
    use crate::providers::{detect_provider_kind, resolve_model_alias, ProviderKind};

    #[test]
    fn resolves_existing_openai_gemini_and_grok_aliases() {
        assert_eq!(resolve_model_alias("gpt"), "gpt-5.4");
        assert_eq!(resolve_model_alias("gemini"), "gemini-2.5-pro");
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
    }

    #[test]
    fn provider_detection_prefers_model_family() {
        assert_eq!(detect_provider_kind("gpt-5.4"), ProviderKind::OpenAi);
        assert_eq!(detect_provider_kind("gemini-2.5-pro"), ProviderKind::Gemini);
        assert_eq!(detect_provider_kind("grok-3"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::ClawApi
        );
    }
}
