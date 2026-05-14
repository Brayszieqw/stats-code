use std::env;
use std::path::PathBuf;

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
