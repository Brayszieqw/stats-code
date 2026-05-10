mod error;
mod hooks;
mod loader;
mod manager;
mod plugin;
mod registry;
mod source;
#[cfg(test)]
mod tests;
mod types;

pub use error::{PluginError, PluginManifestValidationError};
pub use hooks::{HookEvent, HookRunResult, HookRunner};
pub use loader::{builtin_plugins, load_plugin_from_directory};
pub use manager::{InstallOutcome, PluginManager, PluginManagerConfig, UpdateOutcome};
pub use plugin::{
    BuiltinPlugin, BundledPlugin, ExternalPlugin, Plugin, PluginDefinition, PluginSummary,
    RegisteredPlugin,
};
pub use registry::PluginRegistry;
pub use types::{
    InstalledPluginRecord, InstalledPluginRegistry, PluginCommandManifest, PluginHooks,
    PluginInstallSource, PluginKind, PluginLifecycle, PluginManifest, PluginMetadata,
    PluginPermission, PluginTool, PluginToolDefinition, PluginToolManifest, PluginToolPermission,
};

const EXTERNAL_MARKETPLACE: &str = "external";
const BUILTIN_MARKETPLACE: &str = "builtin";
const BUNDLED_MARKETPLACE: &str = "bundled";
const SETTINGS_FILE_NAME: &str = "settings.json";
const REGISTRY_FILE_NAME: &str = "installed.json";
const MANIFEST_FILE_NAME: &str = "plugin.json";
const MANIFEST_RELATIVE_PATH: &str = ".claw-plugin/plugin.json";
