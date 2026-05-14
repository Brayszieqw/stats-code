use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cli::AuthProvider;
use crate::helpers::stringify_error;

use super::paths::{stats_code_env_path, stats_code_profile_path};

// ---------------------------------------------------------------------------
// Profile structs
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Load / save functions
// ---------------------------------------------------------------------------

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
// Current profile helpers
// ---------------------------------------------------------------------------

pub(crate) fn current_stats_code_profile() -> StatsCodeProfile {
    load_stats_code_profile(&stats_code_profile_path()).unwrap_or_default()
}

pub(crate) fn current_stats_code_env() -> StatsCodeProfileEnv {
    load_stats_code_env(&stats_code_env_path()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Profile query functions
// ---------------------------------------------------------------------------

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
