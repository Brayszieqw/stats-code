use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use api::{
    detect_provider_kind, max_tokens_for_model, resolve_model_alias, InputMessage, MessageRequest,
    OpenAiCompatClient, OpenAiCompatConfig, OutputContentBlock, ProviderClient, ProviderKind,
};
use serde::{Deserialize, Serialize};

use crate::cli::{AiAskArgs, AuthDoctorArgs, AuthProvider, AuthSetArgs, ConfigModelArgs};
use crate::error::StatsCodeResult;
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::schema::{
    AiAskResult, AuthDoctorProviderStatus, AuthDoctorResult, AuthSetResult, ConfigResult,
};

// ---------------------------------------------------------------------------
// Config handler functions
// ---------------------------------------------------------------------------

pub(crate) fn handle_config_show() -> StatsCodeResult<ConfigResult> {
    let path = stats_code_settings_path();
    let settings = load_stats_code_settings(&path)?;
    let profile_path = stats_code_profile_path();
    let env_path = stats_code_env_path();
    let profile = load_stats_code_profile(&profile_path)?;
    let notes = vec![
        "This config is inspired by Doge Code's separate model/config management.".to_string(),
        "Default model is used when you run `stats code` or `stats-code ai ask` without an explicit --model.".to_string(),
        format!("Profile path: {}", profile_path.display()),
        format!("Secret env path: {}", env_path.display()),
        profile
            .model_provider.map_or_else(|| "Profile model provider: <none>".to_string(), |provider| format!("Profile model provider: {provider}")),
    ];
    Ok(build_config_result(
        "show",
        &path,
        &settings,
        "Loaded Stats Code settings.".to_string(),
        notes,
    ))
}

pub(crate) fn handle_config_default_model(args: &ConfigModelArgs) -> StatsCodeResult<ConfigResult> {
    let path = stats_code_settings_path();
    let mut settings = load_stats_code_settings(&path)?;
    let model = args.model.trim();
    if model.is_empty() {
        return Err("Model cannot be empty.".into());
    }
    settings.default_model = Some(model.to_string());
    push_saved_model(&mut settings.saved_models, model);
    settings.updated_at_unix_nanos = unix_timestamp_nanos();
    save_stats_code_settings(&path, &settings)?;
    Ok(build_config_result(
        "default_model",
        &path,
        &settings,
        format!("Default model set to `{model}`."),
        vec!["The model was also added to the saved model list.".to_string()],
    ))
}

pub(crate) fn handle_config_add_model(args: &ConfigModelArgs) -> StatsCodeResult<ConfigResult> {
    let path = stats_code_settings_path();
    let mut settings = load_stats_code_settings(&path)?;
    let model = args.model.trim();
    if model.is_empty() {
        return Err("Model cannot be empty.".into());
    }
    let already_present = settings.saved_models.iter().any(|value| value == model);
    push_saved_model(&mut settings.saved_models, model);
    settings.updated_at_unix_nanos = unix_timestamp_nanos();
    save_stats_code_settings(&path, &settings)?;
    Ok(build_config_result(
        "add_model",
        &path,
        &settings,
        if already_present {
            format!("Model `{model}` was already in the saved model list.")
        } else {
            format!("Added `{model}` to the saved model list.")
        },
        vec!["Saved models make switching easier across projects and sessions.".to_string()],
    ))
}

pub(crate) fn handle_config_remove_model(args: &ConfigModelArgs) -> StatsCodeResult<ConfigResult> {
    let path = stats_code_settings_path();
    let mut settings = load_stats_code_settings(&path)?;
    let model = args.model.trim();
    if model.is_empty() {
        return Err("Model cannot be empty.".into());
    }
    let before_len = settings.saved_models.len();
    settings.saved_models.retain(|value| value != model);
    if settings.default_model.as_deref() == Some(model) {
        settings.default_model = settings.saved_models.first().cloned();
    }
    settings.updated_at_unix_nanos = unix_timestamp_nanos();
    save_stats_code_settings(&path, &settings)?;
    Ok(build_config_result(
        "remove_model",
        &path,
        &settings,
        if settings.saved_models.len() == before_len {
            format!("Model `{model}` was not present in the saved model list.")
        } else {
            format!("Removed `{model}` from the saved model list.")
        },
        vec![
            "If the removed model was the default, Stats Code promotes the first remaining saved model as the new default.".to_string(),
        ],
    ))
}

fn build_config_result(
    action: &str,
    path: &Path,
    settings: &StatsCodeSettings,
    message: String,
    notes: Vec<String>,
) -> ConfigResult {
    ConfigResult {
        status: "ok".to_string(),
        action: action.to_string(),
        config_path: path.display().to_string(),
        default_model: settings.default_model.clone(),
        saved_models: settings.saved_models.clone(),
        message,
        notes,
    }
}

fn push_saved_model(saved_models: &mut Vec<String>, model: &str) {
    if !saved_models.iter().any(|value| value == model) {
        saved_models.push(model.to_string());
    }
}

pub(crate) fn resolve_requested_model(requested: &str) -> String {
    if requested != "gpt" {
        return requested.to_string();
    }
    let settings = load_stats_code_settings(&stats_code_settings_path()).unwrap_or_default();
    profile_default_model()
        .or_else(|| {
            settings
                .default_model
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| requested.to_string())
}

// ---------------------------------------------------------------------------
// Auth handler functions
// ---------------------------------------------------------------------------

pub(crate) fn handle_auth_set(args: &AuthSetArgs) -> StatsCodeResult<AuthSetResult> {
    let config_path = stats_code_auth_path();
    let mut store = load_auth_store(&config_path)?;
    let api_key = args.api_key.trim();
    if api_key.is_empty() {
        return Err("`--api-key` cannot be empty.".into());
    }

    store.providers.insert(
        args.provider.store_key().to_string(),
        StoredProviderCredential {
            api_key: api_key.to_string(),
            base_url: args
                .base_url
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            updated_at_unix_nanos: unix_timestamp_nanos(),
        },
    );
    save_auth_store(&config_path, &store)?;

    Ok(AuthSetResult {
        status: "ok".to_string(),
        provider: args.provider.display_name().to_string(),
        config_path: config_path.display().to_string(),
        api_key_env: args.provider.api_key_env().to_string(),
        base_url_env: Some(args.provider.base_url_env().to_string()),
        notes: vec![
            "Credentials were saved to the local Stats Code auth store.".to_string(),
            "Saved credentials are loaded automatically by `stats-code ai ask` when process environment variables are absent.".to_string(),
            "The API key is not shown again after save.".to_string(),
        ],
    })
}

pub(crate) fn handle_auth_doctor(args: &AuthDoctorArgs) -> StatsCodeResult<AuthDoctorResult> {
    let config_path = stats_code_auth_path();
    let store = load_auth_store(&config_path)?;
    let providers = args
        .provider
        .map_or_else(supported_auth_providers, |provider| vec![provider]);
    let provider_statuses = providers
        .into_iter()
        .map(|provider| build_auth_doctor_provider_status(provider, &store))
        .collect::<Vec<_>>();

    let profile_path = stats_code_profile_path();
    let env_path = stats_code_env_path();
    Ok(AuthDoctorResult {
        status: "ok".to_string(),
        config_path: format!(
            "api_keys={} profile={} env={}",
            config_path.display(),
            profile_path.display(),
            env_path.display()
        ),
        providers: provider_statuses,
        notes: vec![
            "Source `process_env` means the current shell already exports the provider env vars."
                .to_string(),
            "Source `profile_config` means Stats Code can load provider defaults from `profile.toml` and `env.json` for the current request."
                .to_string(),
            "Source `saved_config` means Stats Code can load provider credentials from its local auth store at runtime.".to_string(),
        ],
    })
}

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

#[derive(Debug, Clone)]
pub(crate) struct PreparedAiProvider {
    pub(crate) provider_name: String,
    pub(crate) credential_source: String,
    pub(crate) notes: Vec<String>,
    pub(crate) client: ProviderClient,
}

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
        ProviderKind::ClawApi => {
            if has_non_empty_env("ANTHROPIC_API_KEY") || has_non_empty_env("ANTHROPIC_AUTH_TOKEN") {
                Ok(PreparedAiProvider {
                    provider_name: "Anthropic".to_string(),
                    credential_source: "process_env".to_string(),
                    notes: vec![
                        "Claude models currently rely on existing Anthropic environment/OAuth configuration.".to_string(),
                    ],
                    client: ProviderClient::from_model(resolved_model).map_err(|error| {
                        format!("Failed to initialize provider client: {error}")
                    })?,
                })
            } else {
                Err(format!(
                    "Model `{resolved_model}` resolves to Claude/Anthropic. `stats code auth set` does not manage Anthropic yet; export ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN first."
                ))
            }
        }
        _ => Err(format!(
            "Provider for model `{resolved_model}` is not supported by Stats Code auth helpers yet."
        )),
    }
}

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
        ProviderKind::ClawApi => {
            return ProviderClient::from_model(resolved_model)
                .map_err(|error| format!("Failed to initialize provider client: {error}"));
        }
    };
    Ok(client)
}

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

fn build_auth_doctor_provider_status(
    provider: AuthProvider,
    store: &StoredAuthStore,
) -> AuthDoctorProviderStatus {
    let saved = store.providers.get(provider.store_key());
    let profile_provider = profile_provider_config(provider);
    let profile_key = profile_credential_value(provider);
    let api_key_present_in_env = has_non_empty_env(provider.api_key_env());
    let base_url_present_in_env = has_non_empty_env(provider.base_url_env());
    let credential_source = if api_key_present_in_env {
        "process_env"
    } else if profile_key.is_some() {
        "profile_config"
    } else if saved.is_some() {
        "saved_config"
    } else {
        "missing"
    };

    let mut notes = Vec::new();
    if api_key_present_in_env && saved.is_some() {
        notes.push("Process environment overrides the saved auth store.".to_string());
    }
    if saved.is_none() {
        notes.push(format!(
            "Set credentials with `stats-code auth set {} --api-key <key>`.",
            provider.store_key()
        ));
    }
    if profile_key.is_some() {
        notes.push("Stats Code profile config provides this provider credential.".to_string());
    }

    AuthDoctorProviderStatus {
        provider: provider.display_name().to_string(),
        model_hint: provider.model_hint().to_string(),
        api_key_env: provider.api_key_env().to_string(),
        base_url_env: Some(provider.base_url_env().to_string()),
        credential_source: credential_source.to_string(),
        api_key_present: api_key_present_in_env || profile_key.is_some() || saved.is_some(),
        base_url_present: base_url_present_in_env
            || normalized_profile_base_url(provider, profile_provider.as_ref()).is_some()
            || saved
                .and_then(|credential| credential.base_url.as_ref())
                .is_some_and(|value| !value.trim().is_empty()),
        configured_base_url: env::var(provider.base_url_env())
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| normalized_profile_base_url(provider, profile_provider.as_ref()))
            .or_else(|| {
                saved
                    .and_then(|credential| credential.base_url.clone())
                    .filter(|value| !value.trim().is_empty())
            }),
        notes,
    }
}

// ---------------------------------------------------------------------------
// Config path functions
// ---------------------------------------------------------------------------

pub(crate) fn stats_code_auth_path() -> PathBuf {
    stats_code_config_dir().join("auth.json")
}

pub(crate) fn stats_code_profile_path() -> PathBuf {
    stats_code_config_dir().join("profile.toml")
}

pub(crate) fn stats_code_env_path() -> PathBuf {
    stats_code_config_dir().join("env.json")
}

pub(crate) fn stats_code_settings_path() -> PathBuf {
    stats_code_config_dir().join("settings.json")
}

pub(crate) fn stats_code_config_dir() -> PathBuf {
    if let Some(path) = env::var_os("STATS_CODE_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    if cfg!(windows) {
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("StatsCode");
        }
    } else if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg_config_home).join("stats-code");
    }

    home_dir().map_or_else(
        || PathBuf::from(".stats-code"),
        |path| path.join(".stats-code"),
    )
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

// ---------------------------------------------------------------------------
// Load / save functions
// ---------------------------------------------------------------------------

pub(crate) fn load_auth_store(path: &Path) -> Result<StoredAuthStore, String> {
    if !path.is_file() {
        return Ok(StoredAuthStore::default());
    }
    serde_json::from_str(&fs::read_to_string(path).map_err(stringify_error)?)
        .map_err(stringify_error)
}

pub(crate) fn save_auth_store(path: &Path, store: &StoredAuthStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(stringify_error)?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(store).map_err(stringify_error)?,
    )
    .map_err(stringify_error)
}

pub(crate) fn load_stats_code_settings(path: &Path) -> Result<StatsCodeSettings, String> {
    if !path.is_file() {
        return Ok(StatsCodeSettings::default());
    }
    serde_json::from_str(&fs::read_to_string(path).map_err(stringify_error)?)
        .map_err(stringify_error)
}

pub(crate) fn save_stats_code_settings(
    path: &Path,
    settings: &StatsCodeSettings,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(stringify_error)?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(settings).map_err(stringify_error)?,
    )
    .map_err(stringify_error)
}

pub(crate) fn load_stats_code_profile(path: &Path) -> Result<StatsCodeProfile, String> {
    if !path.is_file() {
        return Ok(StatsCodeProfile::default());
    }
    toml::from_str(&fs::read_to_string(path).map_err(stringify_error)?).map_err(stringify_error)
}

#[cfg(test)]
pub(crate) fn save_stats_code_profile(
    path: &Path,
    profile: &StatsCodeProfile,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(stringify_error)?;
    }
    fs::write(
        path,
        toml::to_string_pretty(profile).map_err(stringify_error)?,
    )
    .map_err(stringify_error)
}

pub(crate) fn load_stats_code_env(path: &Path) -> Result<StatsCodeProfileEnv, String> {
    if !path.is_file() {
        return Ok(StatsCodeProfileEnv::default());
    }
    serde_json::from_str(&fs::read_to_string(path).map_err(stringify_error)?)
        .map_err(stringify_error)
}

#[cfg(test)]
pub(crate) fn save_stats_code_env(
    path: &Path,
    profile_env: &StatsCodeProfileEnv,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(stringify_error)?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(profile_env).map_err(stringify_error)?,
    )
    .map_err(stringify_error)
}

// ---------------------------------------------------------------------------
// Profile helper functions
// ---------------------------------------------------------------------------

pub(crate) fn current_stats_code_profile() -> StatsCodeProfile {
    load_stats_code_profile(&stats_code_profile_path()).unwrap_or_default()
}

pub(crate) fn current_stats_code_env() -> StatsCodeProfileEnv {
    load_stats_code_env(&stats_code_env_path()).unwrap_or_default()
}

pub(crate) fn profile_provider_config(provider: AuthProvider) -> Option<StatsCodeProviderProfile> {
    let profile = current_stats_code_profile();
    profile.model_providers.get(provider.profile_key()).cloned()
}

pub(crate) fn profile_credential_value(provider: AuthProvider) -> Option<String> {
    current_stats_code_env()
        .entries
        .get(provider.api_key_env())
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn profile_default_model() -> Option<String> {
    current_stats_code_profile()
        .model
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn normalized_profile_base_url(
    provider: AuthProvider,
    config: Option<&StatsCodeProviderProfile>,
) -> Option<String> {
    let config = config?;
    let raw = config.base_url.as_ref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    if provider == AuthProvider::Openai {
        let wire_api = config
            .wire_api
            .as_deref()
            .unwrap_or("chat_completions")
            .to_ascii_lowercase();
        if !lower.ends_with("/v1")
            && !lower.ends_with("/chat/completions")
            && !lower.ends_with("/responses")
            && (wire_api == "responses" || wire_api == "chat_completions")
        {
            return Some(format!("{raw}/v1"));
        }
    }
    Some(raw.to_string())
}

// ---------------------------------------------------------------------------
// Auth helper functions
// ---------------------------------------------------------------------------

pub(crate) fn has_non_empty_env(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn supported_auth_providers() -> Vec<AuthProvider> {
    vec![
        AuthProvider::Openai,
        AuthProvider::Gemini,
        AuthProvider::Deepseek,
        AuthProvider::Dashscope,
        AuthProvider::Moonshot,
        AuthProvider::Xai,
    ]
}

pub(crate) fn auth_provider_from_kind(kind: ProviderKind) -> Option<AuthProvider> {
    match kind {
        ProviderKind::OpenAi => Some(AuthProvider::Openai),
        ProviderKind::Gemini => Some(AuthProvider::Gemini),
        ProviderKind::DeepSeek => Some(AuthProvider::Deepseek),
        ProviderKind::DashScope => Some(AuthProvider::Dashscope),
        ProviderKind::Moonshot => Some(AuthProvider::Moonshot),
        ProviderKind::Xai => Some(AuthProvider::Xai),
        ProviderKind::ClawApi => None,
    }
}

pub(crate) fn parse_auth_provider_name(value: &str) -> Option<AuthProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "gpt" => Some(AuthProvider::Openai),
        "gemini" => Some(AuthProvider::Gemini),
        "deepseek" => Some(AuthProvider::Deepseek),
        "dashscope" => Some(AuthProvider::Dashscope),
        "moonshot" => Some(AuthProvider::Moonshot),
        "xai" => Some(AuthProvider::Xai),
        _ => None,
    }
}

pub(crate) fn estimate_session_cost_usd(
    pricing: &BTreeMap<String, ModelPricing>,
    model: &str,
    usage: &ChatUsageTotals,
) -> Option<f64> {
    let resolved = resolve_model_alias(model);
    let pricing = pricing.get(&resolved).or_else(|| pricing.get(model))?;
    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * pricing.input_per_million_usd;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_million_usd;
    Some(input_cost + output_cost)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedChatSession {
    pub(crate) version: u32,
    pub(crate) cwd: String,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) system: Option<String>,
    #[serde(default)]
    pub(crate) max_tokens: Option<u32>,
    pub(crate) use_tools: bool,
    #[serde(default)]
    pub(crate) fast_mode: bool,
    #[serde(default)]
    pub(crate) vim_mode: bool,
    #[serde(default)]
    pub(crate) messages: Vec<InputMessage>,
    #[serde(default)]
    pub(crate) input_tokens_total: u64,
    #[serde(default)]
    pub(crate) output_tokens_total: u64,
    #[serde(default)]
    pub(crate) tool_calls_total: u64,
    #[serde(default)]
    pub(crate) turns_total: u64,
    #[serde(default)]
    pub(crate) last_request_id: Option<String>,
    pub(crate) updated_at_unix_nanos: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StatsCodeSettings {
    #[serde(default)]
    pub(crate) default_model: Option<String>,
    #[serde(default)]
    pub(crate) saved_models: Vec<String>,
    #[serde(default)]
    pub(crate) pricing: BTreeMap<String, ModelPricing>,
    #[serde(default)]
    pub(crate) updated_at_unix_nanos: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelPricing {
    pub(crate) input_per_million_usd: f64,
    pub(crate) output_per_million_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StatsCodeProfile {
    #[serde(default)]
    pub(crate) model_provider: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) review_model: Option<String>,
    #[serde(default)]
    pub(crate) model_reasoning_effort: Option<String>,
    #[serde(default)]
    pub(crate) disable_response_storage: Option<bool>,
    #[serde(default)]
    pub(crate) network_access: Option<String>,
    #[serde(default)]
    pub(crate) windows_wsl_setup_acknowledged: Option<bool>,
    #[serde(default)]
    pub(crate) model_context_window: Option<u64>,
    #[serde(default)]
    pub(crate) model_auto_compact_token_limit: Option<u64>,
    #[serde(default)]
    pub(crate) model_providers: BTreeMap<String, StatsCodeProviderProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StatsCodeProviderProfile {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
    #[serde(default)]
    pub(crate) wire_api: Option<String>,
    #[serde(default)]
    pub(crate) requires_openai_auth: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StatsCodeProfileEnv {
    #[serde(flatten)]
    pub(crate) entries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StoredAuthStore {
    #[serde(default)]
    pub(crate) providers: BTreeMap<String, StoredProviderCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredProviderCredential {
    pub(crate) api_key: String,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
    pub(crate) updated_at_unix_nanos: u128,
}

// ---------------------------------------------------------------------------
// AuthProvider impl (methods moved from handlers.rs)
// ---------------------------------------------------------------------------

impl AuthProvider {
    pub(crate) fn store_key(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Gemini => "gemini",
            Self::Deepseek => "deepseek",
            Self::Dashscope => "dashscope",
            Self::Moonshot => "moonshot",
            Self::Xai => "xai",
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Openai => "OpenAI",
            Self::Gemini => "Gemini",
            Self::Deepseek => "DeepSeek",
            Self::Dashscope => "DashScope",
            Self::Moonshot => "Moonshot",
            Self::Xai => "xAI",
        }
    }

    pub(crate) fn profile_key(self) -> &'static str {
        match self {
            Self::Openai => "OpenAI",
            Self::Gemini => "Gemini",
            Self::Deepseek => "DeepSeek",
            Self::Dashscope => "DashScope",
            Self::Moonshot => "Moonshot",
            Self::Xai => "xAI",
        }
    }

    pub(crate) fn api_key_env(self) -> &'static str {
        match self {
            Self::Openai => "OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Deepseek => "DEEPSEEK_API_KEY",
            Self::Dashscope => "DASHSCOPE_API_KEY",
            Self::Moonshot => "MOONSHOT_API_KEY",
            Self::Xai => "XAI_API_KEY",
        }
    }

    pub(crate) fn base_url_env(self) -> &'static str {
        match self {
            Self::Openai => "OPENAI_BASE_URL",
            Self::Gemini => "GEMINI_BASE_URL",
            Self::Deepseek => "DEEPSEEK_BASE_URL",
            Self::Dashscope => "DASHSCOPE_BASE_URL",
            Self::Moonshot => "MOONSHOT_BASE_URL",
            Self::Xai => "XAI_BASE_URL",
        }
    }

    pub(crate) fn model_hint(self) -> &'static str {
        match self {
            Self::Openai => "gpt",
            Self::Gemini => "gemini",
            Self::Deepseek => "deepseek",
            Self::Dashscope => "qwen",
            Self::Moonshot => "moonshot",
            Self::Xai => "grok",
        }
    }
}

// ---------------------------------------------------------------------------
// Chat usage totals (used by both config and chat modules)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct ChatUsageTotals {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) tool_calls: u64,
    pub(crate) turns: u64,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;
    use crate::cli::Cli;
    use crate::cli::{
        AuthCommand, AuthDoctorArgs, AuthProvider, AuthSetArgs, Command, ConfigCommand,
        ConfigModelArgs,
    };
    use crate::handlers::dispatch;

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("epistat-{label}-{nanos}"))
    }

    fn test_cli(command: Command) -> Cli {
        Cli {
            json: false,
            artifacts_dir: None,
            session: None,
            model: "gpt".to_string(),
            system: None,
            max_tokens: None,
            engine: crate::bridge::Engine::Rust,
            command: Some(command),
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let original = std::env::var(key).ok();
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn env_test_guard() -> MutexGuard<'static, ()> {
        static ENV_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env test mutex poisoned")
    }

    #[test]
    fn auth_store_round_trip_persists_saved_credentials() {
        let root = temp_dir("auth-store");
        fs::create_dir_all(&root).expect("create root");
        let auth_path = root.join("auth.json");
        let mut store = StoredAuthStore::default();
        store.providers.insert(
            "openai".to_string(),
            StoredProviderCredential {
                api_key: "sk-test".to_string(),
                base_url: Some("https://example.invalid/v1".to_string()),
                updated_at_unix_nanos: 42,
            },
        );

        save_auth_store(&auth_path, &store).expect("save auth store");
        let loaded = load_auth_store(&auth_path).expect("load auth store");
        let openai = loaded.providers.get("openai").expect("openai credential");
        assert_eq!(openai.api_key, "sk-test");
        assert_eq!(
            openai.base_url.as_deref(),
            Some("https://example.invalid/v1")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn auth_commands_save_and_report_saved_credentials() {
        let _env_guard = env_test_guard();
        let root = temp_dir("auth-cli");
        fs::create_dir_all(&root).expect("create root");
        let _config_guard = EnvVarGuard::set(
            "STATS_CODE_CONFIG_DIR",
            Some(root.to_str().expect("utf8 path")),
        );
        let _openai_key_guard = EnvVarGuard::set("OPENAI_API_KEY", None);
        let _openai_base_guard = EnvVarGuard::set("OPENAI_BASE_URL", None);

        let set_cli = test_cli(Command::Auth {
            command: AuthCommand::Set(AuthSetArgs {
                provider: AuthProvider::Openai,
                api_key: "sk-test".to_string(),
                base_url: Some("https://example.invalid/v1".to_string()),
            }),
        });
        let rendered = dispatch(&set_cli).expect("auth set should succeed");
        assert!(rendered.contains("Auth Set"));

        let doctor_cli = test_cli(Command::Auth {
            command: AuthCommand::Doctor(AuthDoctorArgs {
                provider: Some(AuthProvider::Openai),
            }),
        });
        let rendered = dispatch(&doctor_cli).expect("auth doctor should succeed");
        assert!(rendered.contains("source=saved_config"));
        assert!(rendered.contains("configured_base_url=https://example.invalid/v1"));
        assert!(root.join("auth.json").is_file());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn config_commands_persist_default_and_saved_models() {
        let _env_guard = env_test_guard();
        let root = temp_dir("config-cli");
        fs::create_dir_all(&root).expect("create root");
        let _config_guard = EnvVarGuard::set(
            "STATS_CODE_CONFIG_DIR",
            Some(root.to_str().expect("utf8 path")),
        );

        let add_cli = test_cli(Command::Config {
            command: ConfigCommand::AddModel(ConfigModelArgs {
                model: "gemini-2.5-pro".to_string(),
            }),
        });
        let add_rendered = dispatch(&add_cli).expect("config add-model should succeed");
        assert!(add_rendered.contains("Saved models"));
        assert!(add_rendered.contains("gemini-2.5-pro"));

        let default_cli = test_cli(Command::Config {
            command: ConfigCommand::DefaultModel(ConfigModelArgs {
                model: "gemini-2.5-pro".to_string(),
            }),
        });
        let default_rendered = dispatch(&default_cli).expect("config default-model should succeed");
        assert!(default_rendered.contains("Default model"));
        assert!(default_rendered.contains("gemini-2.5-pro"));

        let show_cli = test_cli(Command::Config {
            command: ConfigCommand::Show,
        });
        let show_rendered = dispatch(&show_cli).expect("config show should succeed");
        assert!(show_rendered.contains("Loaded Stats Code settings"));
        assert!(show_rendered.contains("gemini-2.5-pro"));

        let settings =
            load_stats_code_settings(&stats_code_settings_path()).expect("load settings");
        assert_eq!(settings.default_model.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!(resolve_requested_model("gpt"), "gemini-2.5-pro");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn profile_files_can_persist_opencode_style_defaults() {
        let _env_guard = env_test_guard();
        let root = temp_dir("profile-cli");
        fs::create_dir_all(&root).expect("create root");
        let _config_guard = EnvVarGuard::set(
            "STATS_CODE_CONFIG_DIR",
            Some(root.to_str().expect("utf8 path")),
        );

        let mut profile = StatsCodeProfile {
            model_provider: Some("OpenAI".to_string()),
            model: Some("gpt-5.4".to_string()),
            review_model: Some("gpt-5.4".to_string()),
            model_reasoning_effort: Some("xhigh".to_string()),
            disable_response_storage: Some(true),
            network_access: Some("enabled".to_string()),
            windows_wsl_setup_acknowledged: Some(true),
            model_context_window: Some(1_000_000),
            model_auto_compact_token_limit: Some(900_000),
            model_providers: BTreeMap::new(),
        };
        profile.model_providers.insert(
            "OpenAI".to_string(),
            StatsCodeProviderProfile {
                name: Some("OpenAI".to_string()),
                base_url: Some("https://gmn.chuangzuoli.com".to_string()),
                wire_api: Some("responses".to_string()),
                requires_openai_auth: Some(true),
            },
        );
        save_stats_code_profile(&stats_code_profile_path(), &profile).expect("save profile");
        save_stats_code_env(
            &stats_code_env_path(),
            &StatsCodeProfileEnv {
                entries: BTreeMap::from([(
                    "OPENAI_API_KEY".to_string(),
                    "sk-test-profile".to_string(),
                )]),
            },
        )
        .expect("save profile env");

        let loaded_profile =
            load_stats_code_profile(&stats_code_profile_path()).expect("load profile");
        assert_eq!(loaded_profile.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(resolve_requested_model("gpt"), "gpt-5.4");

        let rendered = dispatch(&test_cli(Command::Auth {
            command: AuthCommand::Doctor(AuthDoctorArgs {
                provider: Some(AuthProvider::Openai),
            }),
        }))
        .expect("auth doctor should succeed");
        assert!(rendered.contains("source=profile_config"));
        assert!(rendered.contains("configured_base_url=https://gmn.chuangzuoli.com/v1"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn prepare_ai_provider_uses_saved_credentials_without_mutating_env() {
        let _env_guard = env_test_guard();
        let root = temp_dir("prepare-ai-provider-saved");
        fs::create_dir_all(&root).expect("create root");
        let _config_guard = EnvVarGuard::set(
            "STATS_CODE_CONFIG_DIR",
            Some(root.to_str().expect("utf8 path")),
        );
        let _openai_key_guard = EnvVarGuard::set("OPENAI_API_KEY", None);
        let _openai_base_guard = EnvVarGuard::set("OPENAI_BASE_URL", None);

        let mut store = StoredAuthStore::default();
        store.providers.insert(
            "openai".to_string(),
            StoredProviderCredential {
                api_key: "sk-test-saved".to_string(),
                base_url: Some("https://example.invalid/v1".to_string()),
                updated_at_unix_nanos: 42,
            },
        );
        save_auth_store(&stats_code_auth_path(), &store).expect("save auth store");

        let prepared =
            prepare_ai_provider(ProviderKind::OpenAi, "gpt-5.4").expect("prepare provider");
        assert_eq!(prepared.credential_source, "saved_config");
        assert_eq!(prepared.client.provider_kind(), ProviderKind::OpenAi);
        assert_eq!(env::var("OPENAI_API_KEY").ok(), None);
        assert_eq!(env::var("OPENAI_BASE_URL").ok(), None);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn estimates_cost_from_pricing_table() {
        let mut pricing = BTreeMap::new();
        pricing.insert(
            "gpt-5.4".to_string(),
            ModelPricing {
                input_per_million_usd: 5.0,
                output_per_million_usd: 15.0,
            },
        );
        let usage = ChatUsageTotals {
            input_tokens: 100_000,
            output_tokens: 20_000,
            tool_calls: 0,
            turns: 1,
        };

        let cost = estimate_session_cost_usd(&pricing, "gpt", &usage).expect("cost estimate");
        assert!((cost - 0.8).abs() < 1e-9);
    }
}
