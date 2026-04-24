//! Chat session persistence — save/load and session path computation.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{stats_code_config_dir, SavedChatSession};
use crate::helpers::{fnv1a64_hex, stringify_error, unix_timestamp_nanos};

use super::ChatSessionState;

pub(crate) fn load_chat_session(path: &Path) -> Result<Option<SavedChatSession>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let saved = serde_json::from_str::<SavedChatSession>(
        &fs::read_to_string(path).map_err(stringify_error)?,
    )
    .map_err(stringify_error)?;
    Ok(Some(saved))
}

pub(crate) fn save_chat_session(state: &ChatSessionState) -> Result<(), String> {
    let saved = SavedChatSession {
        version: 1,
        cwd: state.project_context.cwd.display().to_string(),
        model: state.model.clone(),
        system: state.system.clone(),
        max_tokens: state.max_tokens,
        use_tools: state.use_tools,
        fast_mode: state.fast_mode,
        vim_mode: state.vim_mode,
        messages: state.messages.clone(),
        input_tokens_total: state.usage.input_tokens,
        output_tokens_total: state.usage.output_tokens,
        tool_calls_total: state.usage.tool_calls,
        turns_total: state.usage.turns,
        last_request_id: state.last_request_id.clone(),
        updated_at_unix_nanos: unix_timestamp_nanos(),
    };
    if let Some(parent) = state.session_path.parent() {
        fs::create_dir_all(parent).map_err(stringify_error)?;
    }
    fs::write(
        &state.session_path,
        // P2: Use compact JSON (not pretty-print) for smaller files and faster writes
        serde_json::to_string(&saved).map_err(stringify_error)?,
    )
    .map_err(stringify_error)
}

fn stats_code_sessions_dir() -> PathBuf {
    stats_code_config_dir().join("sessions")
}

pub(crate) fn default_chat_session_path(cwd: &Path) -> PathBuf {
    let display = cwd.display().to_string();
    let hash = fnv1a64_hex(display.as_bytes());
    let stem = cwd
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| "workspace".to_string(), sanitize_session_name);
    stats_code_sessions_dir().join(format!("{stem}-{hash}.json"))
}

fn sanitize_session_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "workspace".to_string()
    } else {
        sanitized
    }
}
