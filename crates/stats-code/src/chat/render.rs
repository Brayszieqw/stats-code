//! Render / display helpers for the chat REPL.
//!
//! Extracted from `chat/mod.rs` to keep that file under 400 lines.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use api::resolve_model_alias;
use serde_json::Value;

use crate::config::{home_dir, load_stats_code_settings, stats_code_settings_path};
use crate::helpers::stringify_error;

use super::discovery::{collect_plugin_manifests, nearest_project_config_dir};
use super::display::format_token_count;
use super::{ChatSessionState, BUILTIN_SLASH_COMMANDS};

// ---------------------------------------------------------------------------
// /status
// ---------------------------------------------------------------------------

pub(super) fn render_status_report(state: &ChatSessionState) -> Result<String, String> {
    let settings = load_stats_code_settings(&stats_code_settings_path())?;
    let resolved_model = resolve_model_alias(&state.model);
    let mut out = String::new();
    let _ = writeln!(out, "Status");
    let _ = writeln!(out, "  Session path      {}", state.session_path.display());
    let _ = writeln!(
        out,
        "  Project cwd       {}",
        state.project_context.cwd.display()
    );
    let _ = writeln!(
        out,
        "  Model             {} -> {}",
        state.model, resolved_model
    );
    let _ = writeln!(
        out,
        "  Modes             tools={} fast={} vim={}",
        if state.use_tools { "on" } else { "off" },
        if state.fast_mode { "on" } else { "off" },
        if state.vim_mode { "on" } else { "off" }
    );
    let _ = writeln!(
        out,
        "  Context files     {}",
        state.project_context.files.len()
    );
    let _ = writeln!(
        out,
        "  Session usage     in={} out={} turns={} tools={}",
        format_token_count(state.usage.input_tokens),
        format_token_count(state.usage.output_tokens),
        state.usage.turns,
        state.usage.tool_calls
    );
    let _ = writeln!(
        out,
        "  Last request id   {}",
        state.last_request_id.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(
        out,
        "  Saved models      {}",
        if settings.saved_models.is_empty() {
            "<none>".to_string()
        } else {
            settings.saved_models.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  Default model     {}",
        settings.default_model.as_deref().unwrap_or("<none>")
    );
    let _ = writeln!(out, "  Pricing entries   {}", settings.pricing.len());
    Ok(out)
}

// ---------------------------------------------------------------------------
// /help (builtin slash commands)
// ---------------------------------------------------------------------------

pub(super) fn render_builtin_slash_help() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "  Slash commands:");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "┌──────────────────┬──────────────────────────────────┐"
    );
    let _ = writeln!(
        out,
        "│   命令             │            说明                │"
    );
    let _ = writeln!(
        out,
        "├──────────────────┼──────────────────────────────────┤"
    );
    for command in BUILTIN_SLASH_COMMANDS {
        let label = if command.args.is_empty() {
            format!("/{}", command.name)
        } else {
            format!("/{} {}", command.name, command.args)
        };
        let _ = writeln!(out, "│{:<15}   │{:<32}  │", label, command.description_zh);
        let _ = writeln!(
            out,
            "├──────────────────┼──────────────────────────────────┤"
        );
    }
    let _ = writeln!(
        out,
        "提示：输入 `/` 会弹出这组内置命令，不再自动混入自定义 slash command。"
    );
    out
}

// ---------------------------------------------------------------------------
// /plugin
// ---------------------------------------------------------------------------

pub(super) fn render_plugin_overview(cwd: &Path) -> Result<String, String> {
    let mut manifests = Vec::new();
    if let Some(home) = home_dir() {
        collect_plugin_manifests(&home.join(".stats-code").join("plugins"), &mut manifests)?;
    }
    if let Some(project_config_dir) = nearest_project_config_dir(cwd) {
        collect_plugin_manifests(&project_config_dir.join("plugins"), &mut manifests)?;
    }

    manifests.sort();
    manifests.dedup();

    let mut rows = Vec::new();
    for manifest_path in manifests {
        let text = fs::read_to_string(&manifest_path).map_err(stringify_error)?;
        let json = serde_json::from_str::<Value>(&text).map_err(stringify_error)?;
        let name = json.get("name").and_then(Value::as_str).unwrap_or("plugin");
        let version = json
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let commands = json
            .get("commands")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let skills = json
            .get("skills")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        rows.push(format!(
            "- {} v{}  commands={} skills={}  {}",
            name,
            version,
            commands,
            skills,
            manifest_path.display()
        ));
    }

    let mut out = String::new();
    let _ = writeln!(out, "Plugin");
    if rows.is_empty() {
        let _ = writeln!(out, "  <none found>");
    } else {
        let _ = writeln!(out, "  found={}", rows.len());
        for row in rows.into_iter().take(30) {
            let _ = writeln!(out, "{row}");
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// /skill
// ---------------------------------------------------------------------------

pub(super) fn render_skill_overview() -> String {
    let mut roots = Vec::new();
    if let Some(home) = home_dir() {
        roots.push(("agents", home.join(".agents").join("skills")));
        roots.push(("stats-code", home.join(".stats-code").join("skills")));
        roots.push(("codex", home.join(".codex").join("skills")));
    }

    let mut out = String::new();
    let _ = writeln!(out, "Skill");
    for (label, root) in roots {
        let entries = fs::read_dir(&root)
            .ok()
            .into_iter()
            .flat_map(std::iter::Iterator::flatten)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let _ = writeln!(out, "  {}: {}  {}", label, entries.len(), root.display());
        for name in entries.into_iter().take(12) {
            let _ = writeln!(out, "    - {name}");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// /mcp
// ---------------------------------------------------------------------------

pub(super) fn render_mcp_overview() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "MCP");
    let mut files = Vec::new();
    if let Some(home) = home_dir() {
        files.push(home.join(".stats-code").join("config.json"));
        files.push(home.join(".stats-code").join("settings.json"));
        files.push(home.join(".stats-code").join("settings.local.json"));
    }

    let mut discovered = BTreeMap::<String, usize>::new();
    for path in files {
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for token in extract_mcp_server_tokens(&content) {
            *discovered.entry(token).or_default() += 1;
        }
    }

    if discovered.is_empty() {
        let _ = writeln!(
            out,
            "  No MCP server names were inferred from local Stats Code config files."
        );
        let _ = writeln!(
            out,
            "  Checked ~/.stats-code/config.json, settings.json, settings.local.json"
        );
        return out;
    }

    let _ = writeln!(
        out,
        "  discovered={}  source=~/.stats-code/*.json",
        discovered.len()
    );
    for (name, hits) in discovered {
        let _ = writeln!(out, "  - {name}  references={hits}");
    }
    out
}

// ---------------------------------------------------------------------------
// MCP token extraction helper
// ---------------------------------------------------------------------------

pub(super) fn extract_mcp_server_tokens(content: &str) -> Vec<String> {
    let mut names = std::collections::BTreeSet::<String>::new();
    let needle = "mcp__";
    let mut start = 0usize;
    while let Some(found) = content[start..].find(needle) {
        let absolute = start + found + needle.len();
        let remainder = &content[absolute..];
        let Some(end) = remainder.find("__") else {
            break;
        };
        let candidate = &remainder[..end];
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            names.insert(candidate.to_string());
        }
        start = absolute + end + 2;
    }
    names.into_iter().collect()
}
