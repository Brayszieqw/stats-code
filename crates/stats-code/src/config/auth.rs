use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

use api::ProviderKind;
use serde::{Deserialize, Serialize};

use crate::cli::{AuthDoctorArgs, AuthProvider, AuthSetArgs};
use crate::error::StatsCodeResult;
use crate::helpers::{stringify_error, unix_timestamp_nanos};
use crate::schema::{AuthDoctorProviderStatus, AuthDoctorResult, AuthSetResult};

// These will resolve via super:: once mod.rs wires everything together (task 2.8).
// For now they reference sibling module functions that will exist in profile.rs and paths.rs.
use super::paths::{stats_code_auth_path, stats_code_env_path, stats_code_profile_path};
use super::profile::{normalized_profile_base_url, profile_credential_value, profile_provider_config};

// ---------------------------------------------------------------------------
// Stored types
// ---------------------------------------------------------------------------

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
// AuthProvider impl (enum defined in cli.rs)
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

// ---------------------------------------------------------------------------
// Load / save auth store
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
        ProviderKind::Anthropic => None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

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
