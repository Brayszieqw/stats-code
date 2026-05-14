mod ai_provider;
mod auth;
mod handlers;
mod paths;
mod pricing;
mod profile;
mod settings;

// Re-export everything to preserve the public API surface of the old config.rs.
#[allow(unused_imports)]
pub(crate) use paths::{
    home_dir, stats_code_auth_path, stats_code_config_dir, stats_code_env_path,
    stats_code_profile_path, stats_code_settings_path,
};

// auth.rs
#[allow(unused_imports)]
pub(crate) use auth::{
    auth_provider_from_kind, handle_auth_doctor, handle_auth_set, has_non_empty_env,
    load_auth_store, parse_auth_provider_name, save_auth_store, supported_auth_providers,
    StoredAuthStore, StoredProviderCredential,
};

// profile.rs
#[allow(unused_imports)]
pub(crate) use profile::{
    current_stats_code_env, current_stats_code_profile, load_stats_code_env,
    load_stats_code_profile, normalized_profile_base_url, profile_credential_value,
    profile_default_model, profile_provider_config, StatsCodeProfile, StatsCodeProfileEnv,
    StatsCodeProviderProfile,
};
#[cfg(test)]
pub(crate) use profile::{save_stats_code_env, save_stats_code_profile};

// settings.rs
#[allow(unused_imports)]
pub(crate) use settings::{load_stats_code_settings, save_stats_code_settings, StatsCodeSettings};

// pricing.rs
#[allow(unused_imports)]
pub(crate) use pricing::{
    estimate_session_cost_usd, ChatUsageTotals, ModelPricing, SavedChatSession,
};

// ai_provider.rs
#[allow(unused_imports)]
pub(crate) use ai_provider::{
    extract_response_text, handle_ai_ask, prepare_ai_provider, PreparedAiProvider,
};

// handlers.rs
#[allow(unused_imports)]
pub(crate) use handlers::{
    handle_config_add_model, handle_config_default_model, handle_config_remove_model,
    handle_config_show, resolve_requested_model,
};

#[cfg(test)]
mod tests;
