// ---------------------------------------------------------------------------
// Slash command and plugin discovery.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::home_dir;
use crate::helpers::stringify_error;

use super::{SlashCommandTemplate, BUILTIN_SLASH_COMMANDS};

// ---------------------------------------------------------------------------
// Memory file helpers
// ---------------------------------------------------------------------------

pub(crate) fn primary_memory_file_path(cwd: &Path) -> PathBuf {
    let stats_md = cwd.join("STATS.md");
    if stats_md.is_file() {
        return stats_md;
    }
    stats_md
}

pub(crate) fn append_memory_note(path: &Path, note: &str) -> Result<(), String> {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return Err("Usage: /memory add <text>".to_string());
    }

    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.contains("## Session Memory") {
        if !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str("## Session Memory\n");
    }
    let _ = writeln!(updated, "- {trimmed}");

    fs::write(path, updated).map_err(stringify_error)
}

// ---------------------------------------------------------------------------
// Markdown / template parsing
// ---------------------------------------------------------------------------

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(stringify_error)? {
        let entry = entry.map_err(stringify_error)?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_command_template(contents: &str) -> (Option<String>, String) {
    let trimmed = contents.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some((frontmatter, body)) = rest.split_once("\n---\n") {
            let description = frontmatter
                .lines()
                .find_map(|line| line.trim().strip_prefix("description:"))
                .map(str::trim)
                .map(|value| value.trim_matches('"').trim_matches('\'').to_string())
                .filter(|value| !value.is_empty());
            return (description, body.trim().to_string());
        }
    }
    (None, trimmed.trim().to_string())
}

fn command_name_from_relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let stem = relative.with_extension("");
    let name = stem
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/");
    if name.is_empty() || name.eq_ignore_ascii_case("README") {
        None
    } else {
        Some(name)
    }
}

// ---------------------------------------------------------------------------
// Plugin discovery
// ---------------------------------------------------------------------------

pub(super) fn nearest_project_config_dir(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .map(|ancestor| ancestor.join(".stats-code"))
        .find(|path| path.is_dir())
}

fn plugin_command_roots(plugins_dir: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut manifests = Vec::new();
    collect_plugin_manifests(plugins_dir, &mut manifests)?;

    let mut roots = BTreeMap::new();
    for manifest_path in manifests {
        let Some(manifest_dir) = manifest_path.parent() else {
            continue;
        };
        let plugin_root = if manifest_dir
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == ".stats-code-plugin")
        {
            manifest_dir
                .parent()
                .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
        } else {
            manifest_dir.to_path_buf()
        };
        let plugin_name = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| {
                value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .or_else(|| {
                plugin_root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "plugin".to_string());

        for candidate in [
            plugin_root.join("commands"),
            plugin_root.join(".stats-code-plugin").join("commands"),
        ] {
            if candidate.is_dir() {
                roots
                    .entry(candidate)
                    .or_insert_with(|| plugin_name.clone());
            }
        }
    }

    Ok(roots.into_iter().collect())
}

pub(super) fn collect_plugin_manifests(
    root: &Path,
    manifests: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(stringify_error)? {
        let entry = entry.map_err(stringify_error)?;
        let path = entry.path();
        if path.is_dir() {
            collect_plugin_manifests(&path, manifests)?;
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("plugin.json"))
        {
            manifests.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Slash command template discovery
// ---------------------------------------------------------------------------

pub(crate) fn discover_slash_command_templates(
    cwd: &Path,
) -> Result<Vec<SlashCommandTemplate>, String> {
    let mut discovered = BTreeMap::<String, SlashCommandTemplate>::new();

    if let Some(project_config) = nearest_project_config_dir(cwd) {
        let commands_root = project_config.join("commands");
        let mut files = Vec::new();
        collect_markdown_files(&commands_root, &mut files)?;
        for path in files {
            let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                continue;
            };
            let (description, body) =
                parse_command_template(&fs::read_to_string(&path).map_err(stringify_error)?);
            discovered
                .entry(name.clone())
                .or_insert(SlashCommandTemplate {
                    name,
                    _description: description,
                    body,
                    path,
                    source: "project .stats-code/commands".to_string(),
                });
        }
    }

    if let Some(home) = home_dir() {
        let user_commands_root = home.join(".stats-code").join("commands");
        let mut files = Vec::new();
        collect_markdown_files(&user_commands_root, &mut files)?;
        for path in files {
            let Some(name) = command_name_from_relative_path(&user_commands_root, &path) else {
                continue;
            };
            let (description, body) =
                parse_command_template(&fs::read_to_string(&path).map_err(stringify_error)?);
            discovered
                .entry(name.clone())
                .or_insert(SlashCommandTemplate {
                    name,
                    _description: description,
                    body,
                    path,
                    source: "user ~/.stats-code/commands".to_string(),
                });
        }

        let user_plugins_root = home.join(".stats-code").join("plugins");
        for (commands_root, plugin_name) in plugin_command_roots(&user_plugins_root)? {
            let mut files = Vec::new();
            collect_markdown_files(&commands_root, &mut files)?;
            for path in files {
                let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                    continue;
                };
                let (description, body) =
                    parse_command_template(&fs::read_to_string(&path).map_err(stringify_error)?);
                discovered
                    .entry(name.clone())
                    .or_insert(SlashCommandTemplate {
                        name,
                        _description: description,
                        body,
                        path,
                        source: format!("plugin:{plugin_name}"),
                    });
            }
        }
    }

    if let Some(project_config) = nearest_project_config_dir(cwd) {
        let project_plugins_root = project_config.join("plugins");
        for (commands_root, plugin_name) in plugin_command_roots(&project_plugins_root)? {
            let mut files = Vec::new();
            collect_markdown_files(&commands_root, &mut files)?;
            for path in files {
                let Some(name) = command_name_from_relative_path(&commands_root, &path) else {
                    continue;
                };
                let (description, body) =
                    parse_command_template(&fs::read_to_string(&path).map_err(stringify_error)?);
                discovered.insert(
                    name.clone(),
                    SlashCommandTemplate {
                        name,
                        _description: description,
                        body,
                        path,
                        source: format!("project-plugin:{plugin_name}"),
                    },
                );
            }
        }
    }

    Ok(discovered.into_values().collect())
}

pub(crate) fn slash_command_completion_candidates(cwd: &Path) -> Vec<String> {
    let _ = cwd;
    BUILTIN_SLASH_COMMANDS
        .iter()
        .map(|command| format!("/{}", command.name))
        .collect()
}

pub(super) fn render_custom_command_prompt(
    template: &SlashCommandTemplate,
    args: &str,
    cwd: &Path,
) -> String {
    let rendered_body = template
        .body
        .replace("$ARGUMENTS", args)
        .replace("{{args}}", args)
        .replace("$CWD", &cwd.display().to_string());

    format!(
        "Execute the slash command `/{}`

Source: {}
Path: {}
Working directory: {}

Command instructions:
{}

User arguments:
{}",
        template.name,
        template.source,
        template.path.display(),
        cwd.display(),
        rendered_body,
        if args.trim().is_empty() {
            "<none>"
        } else {
            args.trim()
        }
    )
}
