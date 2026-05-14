use api::{
    detect_provider_kind, max_tokens_for_model, resolve_model_alias, InputMessage, MessageRequest,
    OpenAiCompatClient, OpenAiCompatConfig, OutputContentBlock, ProviderClient, ProviderKind,
};

use crate::cli::AiAskArgs;
use crate::error::StatsCodeResult;
use crate::helpers::stringify_error;
use crate::schema::AiAskResult;

use super::auth::{
    auth_provider_from_kind, has_non_empty_env, load_auth_store,
};
use super::handlers::resolve_requested_model;
use super::paths::stats_code_auth_path;
use super::profile::{normalized_profile_base_url, profile_credential_value, profile_provider_config};

// ---------------------------------------------------------------------------
// PreparedAiProvider
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct PreparedAiProvider {
    pub(crate) provider_name: String,
    pub(crate) credential_source: String,
    pub(crate) notes: Vec<String>,
    pub(crate) client: ProviderClient,
}

// ---------------------------------------------------------------------------
// prepare_ai_provider
// ---------------------------------------------------------------------------

pub(crate) fn prepare_ai_provider(
    provider_kind: ProviderKind,
    resolved_model: &str,
) -> Result<PreparedAiProvider, String> {
    if let Some(provider) = auth_provider_from_kind(provider_kind) {
        let store = load_auth_store(&stats_code_auth_path())?;
        let saved = store.providers.get(provider.store_key());
        let profile_provider = profile_provider_config(provider);
        let profile_key = profile_credential_value(provider);
        let api_key_present_in_env = has_non_empty_env(provider.api_key_env());
        let base_url_present_in_env = has_non_empty_env(provider.base_url_env());
        let mut notes = Vec::new();
        if api_key_present_in_env {
            if saved.is_some() {
                notes.push(
                    "Process environment credentials override the saved Stats Code auth store."
                        .to_string(),
                );
            }
            if base_url_present_in_env {
                notes.push(format!(
                    "Using custom base URL from {}.",
                    provider.base_url_env()
                ));
            }
            return Ok(PreparedAiProvider {
                provider_name: provider.display_name().to_string(),
                credential_source: "process_env".to_string(),
                notes,
                client: ProviderClient::from_model(resolved_model)
                    .map_err(|error| format!("Failed to initialize provider client: {error}"))?,
            });
        }

        if let Some(profile_key) = profile_key {
            let base_url = normalized_profile_base_url(provider, profile_provider.as_ref());
            notes.push(format!(
                "Loaded {} credentials from Stats Code profile config.",
                provider.display_name()
            ));
            if let Some(wire_api) = profile_provider
                .as_ref()
                .and_then(|config| config.wire_api.as_ref())
                .filter(|value| !value.trim().is_empty())
            {
                notes.push(format!("Profile wire API preference: {wire_api}."));
            }
            if base_url.is_some() {
                notes.push("Using custom base URL from Stats Code profile config.".to_string());
            }
            return Ok(PreparedAiProvider {
                provider_name: provider.display_name().to_string(),
                credential_source: "profile_config".to_string(),
                notes,
                client: build_provider_client_with_overrides(
                    provider_kind,
                    resolved_model,
                    &profile_key,
                    base_url,
                )?,
            });
        }

        let Some(saved) = saved else {
            return Err(format!(
                "Missing {} credentials for model `{resolved_model}`. Run `stats-code auth set {} --api-key <key>` or export {}.",
                provider.display_name(),
                provider.store_key(),
                provider.api_key_env()
            ));
        };
        let base_url =
            normalized_profile_base_url(provider, profile_provider.as_ref()).or_else(|| {
                saved
                    .base_url
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });
        notes.push(format!(
            "Loaded {} credentials from the Stats Code auth store.",
            provider.display_name()
        ));
        if base_url.is_some() {
            notes.push("Using saved custom base URL from the Stats Code auth store.".to_string());
        }
        return Ok(PreparedAiProvider {
            provider_name: provider.display_name().to_string(),
            credential_source: "saved_config".to_string(),
            notes,
            client: build_provider_client_with_overrides(
                provider_kind,
                resolved_model,
                &saved.api_key,
                base_url,
            )?,
        });
    }

    match provider_kind {
        ProviderKind::Anthropic => {
            if has_non_empty_env("ANTHROPIC_API_KEY") || has_non_empty_env("ANTHROPIC_AUTH_TOKEN") {
                Ok(PreparedAiProvider {
                    provider_name: "Anthropic".to_string(),
                    credential_source: "process_env".to_string(),
                    notes: vec![
                        "Anthropic models currently rely on existing Anthropic environment/OAuth configuration.".to_string(),
                    ],
                    client: ProviderClient::from_model(resolved_model).map_err(|error| {
                        format!("Failed to initialize provider client: {error}")
                    })?,
                })
            } else {
                Err(format!(
                    "Model `{resolved_model}` resolves to Anthropic. `stats code auth set` does not manage Anthropic yet; export ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN first."
                ))
            }
        }
        _ => Err(format!(
            "Provider for model `{resolved_model}` is not supported by Stats Code auth helpers yet."
        )),
    }
}

// ---------------------------------------------------------------------------
// build_provider_client_with_overrides
// ---------------------------------------------------------------------------

fn build_provider_client_with_overrides(
    provider_kind: ProviderKind,
    resolved_model: &str,
    api_key: &str,
    base_url: Option<String>,
) -> Result<ProviderClient, String> {
    let base_url = base_url.filter(|value| !value.trim().is_empty());
    let client = match provider_kind {
        ProviderKind::OpenAi => ProviderClient::OpenAi(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::openai(),
            base_url,
        )),
        ProviderKind::Gemini => ProviderClient::Gemini(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::gemini(),
            base_url,
        )),
        ProviderKind::DeepSeek => ProviderClient::DeepSeek(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::deepseek(),
            base_url,
        )),
        ProviderKind::DashScope => ProviderClient::DashScope(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::dashscope(),
            base_url,
        )),
        ProviderKind::Moonshot => ProviderClient::Moonshot(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::moonshot(),
            base_url,
        )),
        ProviderKind::Xai => ProviderClient::Xai(build_openai_compat_client(
            api_key,
            OpenAiCompatConfig::xai(),
            base_url,
        )),
        ProviderKind::Anthropic => {
            return ProviderClient::from_model(resolved_model)
                .map_err(|error| format!("Failed to initialize provider client: {error}"));
        }
    };
    Ok(client)
}

// ---------------------------------------------------------------------------
// build_openai_compat_client
// ---------------------------------------------------------------------------

fn build_openai_compat_client(
    api_key: &str,
    config: OpenAiCompatConfig,
    base_url: Option<String>,
) -> OpenAiCompatClient {
    let client = OpenAiCompatClient::new(api_key.to_string(), config);
    match base_url {
        Some(base_url) => client.with_base_url(base_url),
        None => client,
    }
}

// ---------------------------------------------------------------------------
// extract_response_text
// ---------------------------------------------------------------------------

pub(crate) fn extract_response_text(content: &[OutputContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            OutputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// handle_ai_ask
// ---------------------------------------------------------------------------

pub(crate) fn handle_ai_ask(args: &AiAskArgs) -> StatsCodeResult<AiAskResult> {
    let prompt = args
        .prompt
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("Prompt cannot be empty.".into());
    }

    let requested_model = resolve_requested_model(&args.model);
    let resolved_model = resolve_model_alias(&requested_model);
    let provider_kind = detect_provider_kind(&resolved_model);
    let PreparedAiProvider {
        provider_name,
        credential_source,
        mut notes,
        client,
    } = prepare_ai_provider(provider_kind, &resolved_model)?;
    let request = MessageRequest {
        model: resolved_model.clone(),
        max_tokens: args
            .max_tokens
            .unwrap_or_else(|| max_tokens_for_model(&resolved_model).min(2048)),
        messages: vec![InputMessage::user_text(prompt.clone())],
        system: args.system.clone(),
        tools: None,
        tool_choice: None,
        stream: false,
    };

    let runtime = tokio::runtime::Runtime::new().map_err(stringify_error)?;
    let response = runtime
        .block_on(client.send_message(&request))
        .map_err(|error| format!("AI request failed: {error}"))?;
    let response_text = extract_response_text(&response.content);
    if response_text.is_empty() {
        notes.push("The provider response contained no plain text blocks.".to_string());
    }

    Ok(AiAskResult {
        status: "ok".to_string(),
        provider: provider_name,
        credential_source,
        model: resolved_model,
        prompt,
        response_text: if response_text.is_empty() {
            "<no text response>".to_string()
        } else {
            response_text
        },
        request_id: response.request_id.clone(),
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        total_tokens: response.total_tokens(),
        notes,
    })
}
