mod ai_provider;
mod auth;
mod handlers;
mod paths;
mod pricing;
mod profile;
mod settings;

// Re-export the subset of each submodule's API still consumed across the
// crate. Symbols only referenced by in-crate `#[cfg(test)]` code are gated
// behind `#[cfg(test)]` so a normal lib build does not flag them as dead.

// paths.rs — all path helpers are test-only consumers of the facade.
#[cfg(test)]
pub(crate) use paths::{
    stats_code_auth_path, stats_code_env_path, stats_code_profile_path,
    stats_code_settings_path,
};

// auth.rs
pub(crate) use auth::{handle_auth_doctor, handle_auth_set};
#[cfg(test)]
pub(crate) use auth::{
    load_auth_store, save_auth_store, StoredAuthStore, StoredProviderCredential,
};

// profile.rs
#[cfg(test)]
pub(crate) use profile::{
    load_stats_code_profile, StatsCodeProfile, StatsCodeProfileEnv, StatsCodeProviderProfile,
};
#[cfg(test)]
pub(crate) use profile::{save_stats_code_env, save_stats_code_profile};

// settings.rs
#[cfg(test)]
pub(crate) use settings::load_stats_code_settings;

// ai_provider.rs
pub(crate) use ai_provider::handle_ai_ask;
#[cfg(test)]
pub(crate) use ai_provider::prepare_ai_provider;

// handlers.rs
pub(crate) use handlers::{
    handle_config_add_model, handle_config_default_model, handle_config_remove_model,
    handle_config_show,
};
#[cfg(test)]
pub(crate) use handlers::resolve_requested_model;

#[cfg(test)]
mod tests;
