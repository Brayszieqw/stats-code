use std::path::Path;

use crate::cli::ConfigModelArgs;
use crate::error::StatsCodeResult;
use crate::helpers::unix_timestamp_nanos;
use crate::schema::ConfigResult;

use super::paths::{stats_code_env_path, stats_code_profile_path, stats_code_settings_path};
use super::profile::{load_stats_code_profile, profile_default_model};
use super::settings::{load_stats_code_settings, save_stats_code_settings, StatsCodeSettings};

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
