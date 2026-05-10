use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{PluginError, PluginManifestValidationError};
use crate::plugin::{BuiltinPlugin, BundledPlugin, ExternalPlugin, PluginDefinition};
use crate::types::{
    PluginCommandManifest, PluginHooks, PluginKind, PluginLifecycle, PluginManifest,
    PluginMetadata, PluginPermission, PluginTool, PluginToolDefinition, PluginToolManifest,
    PluginToolPermission, RawPluginManifest, RawPluginToolManifest,
};
use crate::{BUILTIN_MARKETPLACE, MANIFEST_FILE_NAME, MANIFEST_RELATIVE_PATH};

#[must_use]
pub fn builtin_plugins() -> Vec<PluginDefinition> {
    vec![PluginDefinition::Builtin(BuiltinPlugin {
        metadata: PluginMetadata {
            id: plugin_id("example-builtin", BUILTIN_MARKETPLACE),
            name: "example-builtin".to_string(),
            version: "0.1.0".to_string(),
            description: "Example built-in plugin scaffold for the Rust plugin system".to_string(),
            kind: PluginKind::Builtin,
            source: BUILTIN_MARKETPLACE.to_string(),
            default_enabled: false,
            root: None,
        },
        hooks: PluginHooks::default(),
        lifecycle: PluginLifecycle::default(),
        tools: Vec::new(),
    })]
}

pub(crate) fn load_plugin_definition(
    root: &Path,
    kind: PluginKind,
    source: String,
    marketplace: &str,
) -> Result<PluginDefinition, PluginError> {
    let manifest = load_plugin_from_directory(root)?;
    let metadata = PluginMetadata {
        id: plugin_id(&manifest.name, marketplace),
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        kind,
        source,
        default_enabled: manifest.default_enabled,
        root: Some(root.to_path_buf()),
    };
    let hooks = resolve_hooks(root, &manifest.hooks);
    let lifecycle = resolve_lifecycle(root, &manifest.lifecycle);
    let tools = resolve_tools(root, &metadata.id, &metadata.name, &manifest.tools);
    Ok(match kind {
        PluginKind::Builtin => PluginDefinition::Builtin(BuiltinPlugin {
            metadata,
            hooks,
            lifecycle,
            tools,
        }),
        PluginKind::Bundled => PluginDefinition::Bundled(BundledPlugin {
            metadata,
            hooks,
            lifecycle,
            tools,
        }),
        PluginKind::External => PluginDefinition::External(ExternalPlugin {
            metadata,
            hooks,
            lifecycle,
            tools,
        }),
    })
}

pub fn load_plugin_from_directory(root: &Path) -> Result<PluginManifest, PluginError> {
    load_manifest_from_directory(root)
}

fn load_manifest_from_directory(root: &Path) -> Result<PluginManifest, PluginError> {
    let manifest_path = plugin_manifest_path(root)?;
    load_manifest_from_path(root, &manifest_path)
}

fn load_manifest_from_path(
    root: &Path,
    manifest_path: &Path,
) -> Result<PluginManifest, PluginError> {
    let contents = fs::read_to_string(manifest_path).map_err(|error| {
        PluginError::NotFound(format!(
            "plugin manifest not found at {}: {error}",
            manifest_path.display()
        ))
    })?;
    let raw_manifest: RawPluginManifest = serde_json::from_str(&contents)?;
    build_plugin_manifest(root, raw_manifest)
}

pub(crate) fn plugin_manifest_path(root: &Path) -> Result<PathBuf, PluginError> {
    let direct_path = root.join(MANIFEST_FILE_NAME);
    if direct_path.exists() {
        return Ok(direct_path);
    }

    let packaged_path = root.join(MANIFEST_RELATIVE_PATH);
    if packaged_path.exists() {
        return Ok(packaged_path);
    }

    Err(PluginError::NotFound(format!(
        "plugin manifest not found at {} or {}",
        direct_path.display(),
        packaged_path.display()
    )))
}

fn build_plugin_manifest(
    root: &Path,
    raw: RawPluginManifest,
) -> Result<PluginManifest, PluginError> {
    let mut errors = Vec::new();

    validate_required_manifest_field("name", &raw.name, &mut errors);
    validate_required_manifest_field("version", &raw.version, &mut errors);
    validate_required_manifest_field("description", &raw.description, &mut errors);

    let permissions = build_manifest_permissions(&raw.permissions, &mut errors);
    validate_command_entries(root, raw.hooks.pre_tool_use.iter(), "hook", &mut errors);
    validate_command_entries(root, raw.hooks.post_tool_use.iter(), "hook", &mut errors);
    validate_command_entries(
        root,
        raw.lifecycle.init.iter(),
        "lifecycle command",
        &mut errors,
    );
    validate_command_entries(
        root,
        raw.lifecycle.shutdown.iter(),
        "lifecycle command",
        &mut errors,
    );
    let tools = build_manifest_tools(root, raw.tools, &mut errors);
    let commands = build_manifest_commands(root, raw.commands, &mut errors);

    if !errors.is_empty() {
        return Err(PluginError::ManifestValidation(errors));
    }

    Ok(PluginManifest {
        name: raw.name,
        version: raw.version,
        description: raw.description,
        permissions,
        default_enabled: raw.default_enabled,
        hooks: raw.hooks,
        lifecycle: raw.lifecycle,
        tools,
        commands,
    })
}

fn validate_required_manifest_field(
    field: &'static str,
    value: &str,
    errors: &mut Vec<PluginManifestValidationError>,
) {
    if value.trim().is_empty() {
        errors.push(PluginManifestValidationError::EmptyField { field });
    }
}

fn build_manifest_permissions(
    permissions: &[String],
    errors: &mut Vec<PluginManifestValidationError>,
) -> Vec<PluginPermission> {
    let mut seen = BTreeSet::new();
    let mut validated = Vec::new();

    for permission in permissions {
        let permission = permission.trim();
        if permission.is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "permission",
                field: "value",
                name: None,
            });
            continue;
        }
        if !seen.insert(permission.to_string()) {
            errors.push(PluginManifestValidationError::DuplicatePermission {
                permission: permission.to_string(),
            });
            continue;
        }
        match PluginPermission::parse(permission) {
            Some(permission) => validated.push(permission),
            None => errors.push(PluginManifestValidationError::InvalidPermission {
                permission: permission.to_string(),
            }),
        }
    }

    validated
}

fn build_manifest_tools(
    root: &Path,
    tools: Vec<RawPluginToolManifest>,
    errors: &mut Vec<PluginManifestValidationError>,
) -> Vec<PluginToolManifest> {
    let mut seen = BTreeSet::new();
    let mut validated = Vec::new();

    for tool in tools {
        let name = tool.name.trim().to_string();
        if name.is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "tool",
                field: "name",
                name: None,
            });
            continue;
        }
        if !seen.insert(name.clone()) {
            errors.push(PluginManifestValidationError::DuplicateEntry { kind: "tool", name });
            continue;
        }
        if tool.description.trim().is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "tool",
                field: "description",
                name: Some(name.clone()),
            });
        }
        if tool.command.trim().is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "tool",
                field: "command",
                name: Some(name.clone()),
            });
        } else {
            validate_command_entry(root, &tool.command, "tool", errors);
        }
        if !tool.input_schema.is_object() {
            errors.push(PluginManifestValidationError::InvalidToolInputSchema {
                tool_name: name.clone(),
            });
        }
        let Some(required_permission) =
            PluginToolPermission::parse(tool.required_permission.trim())
        else {
            errors.push(
                PluginManifestValidationError::InvalidToolRequiredPermission {
                    tool_name: name.clone(),
                    permission: tool.required_permission.trim().to_string(),
                },
            );
            continue;
        };

        validated.push(PluginToolManifest {
            name,
            description: tool.description,
            input_schema: tool.input_schema,
            command: tool.command,
            args: tool.args,
            required_permission,
        });
    }

    validated
}

fn build_manifest_commands(
    root: &Path,
    commands: Vec<PluginCommandManifest>,
    errors: &mut Vec<PluginManifestValidationError>,
) -> Vec<PluginCommandManifest> {
    let mut seen = BTreeSet::new();
    let mut validated = Vec::new();

    for command in commands {
        let name = command.name.trim().to_string();
        if name.is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "command",
                field: "name",
                name: None,
            });
            continue;
        }
        if !seen.insert(name.clone()) {
            errors.push(PluginManifestValidationError::DuplicateEntry {
                kind: "command",
                name,
            });
            continue;
        }
        if command.description.trim().is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "command",
                field: "description",
                name: Some(name.clone()),
            });
        }
        if command.command.trim().is_empty() {
            errors.push(PluginManifestValidationError::EmptyEntryField {
                kind: "command",
                field: "command",
                name: Some(name.clone()),
            });
        } else {
            validate_command_entry(root, &command.command, "command", errors);
        }
        validated.push(command);
    }

    validated
}

fn validate_command_entries<'a>(
    root: &Path,
    entries: impl Iterator<Item = &'a String>,
    kind: &'static str,
    errors: &mut Vec<PluginManifestValidationError>,
) {
    for entry in entries {
        validate_command_entry(root, entry, kind, errors);
    }
}

fn validate_command_entry(
    root: &Path,
    entry: &str,
    kind: &'static str,
    errors: &mut Vec<PluginManifestValidationError>,
) {
    if entry.trim().is_empty() {
        errors.push(PluginManifestValidationError::EmptyEntryField {
            kind,
            field: "command",
            name: None,
        });
        return;
    }
    if is_literal_command(entry) {
        return;
    }

    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        root.join(entry)
    };
    if !path.exists() {
        errors.push(PluginManifestValidationError::MissingPath { kind, path });
    }
}

fn resolve_hooks(root: &Path, hooks: &PluginHooks) -> PluginHooks {
    PluginHooks {
        pre_tool_use: hooks
            .pre_tool_use
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
        post_tool_use: hooks
            .post_tool_use
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
    }
}

fn resolve_lifecycle(root: &Path, lifecycle: &PluginLifecycle) -> PluginLifecycle {
    PluginLifecycle {
        init: lifecycle
            .init
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
        shutdown: lifecycle
            .shutdown
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
    }
}

fn resolve_tools(
    root: &Path,
    plugin_id: &str,
    plugin_name: &str,
    tools: &[PluginToolManifest],
) -> Vec<PluginTool> {
    tools
        .iter()
        .map(|tool| {
            PluginTool::new(
                plugin_id,
                plugin_name,
                PluginToolDefinition {
                    name: tool.name.clone(),
                    description: Some(tool.description.clone()),
                    input_schema: tool.input_schema.clone(),
                },
                resolve_hook_entry(root, &tool.command),
                tool.args.clone(),
                tool.required_permission,
                Some(root.to_path_buf()),
            )
        })
        .collect()
}

pub(crate) fn validate_hook_paths(
    root: Option<&Path>,
    hooks: &PluginHooks,
) -> Result<(), PluginError> {
    let Some(root) = root else {
        return Ok(());
    };
    for entry in hooks.pre_tool_use.iter().chain(hooks.post_tool_use.iter()) {
        validate_command_path(root, entry, "hook")?;
    }
    Ok(())
}

pub(crate) fn validate_lifecycle_paths(
    root: Option<&Path>,
    lifecycle: &PluginLifecycle,
) -> Result<(), PluginError> {
    let Some(root) = root else {
        return Ok(());
    };
    for entry in lifecycle.init.iter().chain(lifecycle.shutdown.iter()) {
        validate_command_path(root, entry, "lifecycle command")?;
    }
    Ok(())
}

pub(crate) fn validate_tool_paths(
    root: Option<&Path>,
    tools: &[PluginTool],
) -> Result<(), PluginError> {
    let Some(root) = root else {
        return Ok(());
    };
    for tool in tools {
        validate_command_path(root, &tool.command, "tool")?;
    }
    Ok(())
}

fn validate_command_path(root: &Path, entry: &str, kind: &str) -> Result<(), PluginError> {
    if is_literal_command(entry) {
        return Ok(());
    }
    let path = if Path::new(entry).is_absolute() {
        PathBuf::from(entry)
    } else {
        root.join(entry)
    };
    if !path.exists() {
        return Err(PluginError::InvalidManifest(format!(
            "{kind} path `{}` does not exist",
            path.display()
        )));
    }
    Ok(())
}

fn resolve_hook_entry(root: &Path, entry: &str) -> String {
    if is_literal_command(entry) {
        entry.to_string()
    } else {
        root.join(entry).display().to_string()
    }
}

fn is_literal_command(entry: &str) -> bool {
    !entry.starts_with("./") && !entry.starts_with("../") && !Path::new(entry).is_absolute()
}

pub(crate) fn run_lifecycle_commands(
    metadata: &PluginMetadata,
    lifecycle: &PluginLifecycle,
    phase: &str,
    commands: &[String],
) -> Result<(), PluginError> {
    if lifecycle.is_empty() || commands.is_empty() {
        return Ok(());
    }

    for command in commands {
        let mut process = if Path::new(command).exists() {
            if cfg!(windows) {
                let mut process = Command::new("cmd");
                process.arg("/C").arg(command);
                process
            } else {
                let mut process = Command::new("sh");
                process.arg(command);
                process
            }
        } else if cfg!(windows) {
            let mut process = Command::new("cmd");
            process.arg("/C").arg(command);
            process
        } else {
            let mut process = Command::new("sh");
            process.arg("-lc").arg(command);
            process
        };
        if let Some(root) = &metadata.root {
            process.current_dir(root);
        }
        let output = process.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(PluginError::CommandFailed(format!(
                "plugin `{}` {} failed for `{}`: {}",
                metadata.id,
                phase,
                command,
                if stderr.is_empty() {
                    format!("exit status {}", output.status)
                } else {
                    stderr
                }
            )));
        }
    }

    Ok(())
}

pub(crate) fn plugin_id(name: &str, marketplace: &str) -> String {
    format!("{name}@{marketplace}")
}
