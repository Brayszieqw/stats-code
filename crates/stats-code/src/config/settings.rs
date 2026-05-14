use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::helpers::stringify_error;

use super::pricing::ModelPricing;

// ---------------------------------------------------------------------------
// StatsCodeSettings
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Load / Save
// ---------------------------------------------------------------------------

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
