use std::collections::BTreeMap;

use crate::error::PluginError;
use crate::plugin::{PluginSummary, RegisteredPlugin};
use crate::types::{PluginHooks, PluginTool};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginRegistry {
    plugins: Vec<RegisteredPlugin>,
}

impl PluginRegistry {
    #[must_use]
    pub fn new(mut plugins: Vec<RegisteredPlugin>) -> Self {
        plugins.sort_by(|left, right| left.metadata().id.cmp(&right.metadata().id));
        Self { plugins }
    }

    #[must_use]
    pub fn plugins(&self) -> &[RegisteredPlugin] {
        &self.plugins
    }

    #[must_use]
    pub fn get(&self, plugin_id: &str) -> Option<&RegisteredPlugin> {
        self.plugins
            .iter()
            .find(|plugin| plugin.metadata().id == plugin_id)
    }

    #[must_use]
    pub fn contains(&self, plugin_id: &str) -> bool {
        self.get(plugin_id).is_some()
    }

    #[must_use]
    pub fn summaries(&self) -> Vec<PluginSummary> {
        self.plugins.iter().map(RegisteredPlugin::summary).collect()
    }

    pub fn aggregated_hooks(&self) -> Result<PluginHooks, PluginError> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.is_enabled())
            .try_fold(PluginHooks::default(), |acc, plugin| {
                plugin.validate()?;
                Ok(acc.merged_with(plugin.hooks()))
            })
    }

    pub fn aggregated_tools(&self) -> Result<Vec<PluginTool>, PluginError> {
        let mut tools = Vec::new();
        let mut seen_names = BTreeMap::new();
        for plugin in self.plugins.iter().filter(|plugin| plugin.is_enabled()) {
            plugin.validate()?;
            for tool in plugin.tools() {
                if let Some(existing_plugin) =
                    seen_names.insert(tool.definition().name.clone(), tool.plugin_id().to_string())
                {
                    return Err(PluginError::InvalidManifest(format!(
                        "plugin tool `{}` is defined by both `{existing_plugin}` and `{}`",
                        tool.definition().name,
                        tool.plugin_id()
                    )));
                }
                tools.push(tool.clone());
            }
        }
        Ok(tools)
    }

    pub fn initialize(&self) -> Result<(), PluginError> {
        for plugin in self.plugins.iter().filter(|plugin| plugin.is_enabled()) {
            plugin.validate()?;
            plugin.initialize()?;
        }
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), PluginError> {
        for plugin in self
            .plugins
            .iter()
            .rev()
            .filter(|plugin| plugin.is_enabled())
        {
            plugin.shutdown()?;
        }
        Ok(())
    }
}
