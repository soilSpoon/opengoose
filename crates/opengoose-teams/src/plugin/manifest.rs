use std::collections::HashMap;

use serde::Deserialize;

use crate::error::TeamResult;

/// A skill command definition inside a plugin manifest.
///
/// Each entry under `[[skills]]` in `plugin.toml` describes a shell command
/// that the engine can invoke as a skill extension.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginSkillDef {
    /// Skill extension name (must be unique within the plugin).
    pub name: String,
    /// Shell command to execute.
    pub cmd: String,
    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Timeout in seconds for command execution.
    #[serde(default)]
    pub timeout: Option<u64>,
    /// Environment variables to set when running the command.
    #[serde(default)]
    pub envs: HashMap<String, String>,
}

/// Metadata describing a plugin, parsed from `plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Capability tags — e.g. `["skill"]`, `["channel_adapter"]`.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Skill definitions for plugins with the `skill` capability.
    #[serde(default)]
    pub skills: Vec<PluginSkillDef>,
}

impl PluginManifest {
    /// Capabilities joined as a comma-separated string for persistence.
    pub fn capabilities_str(&self) -> String {
        self.capabilities.join(", ")
    }

    /// Whether this plugin declares the `skill` capability.
    pub fn has_skill_capability(&self) -> bool {
        self.capabilities.iter().any(|c| c == "skill")
    }

    /// Whether this plugin declares the `channel_adapter` capability.
    pub fn has_channel_adapter_capability(&self) -> bool {
        self.capabilities.iter().any(|c| c == "channel_adapter")
    }
}

pub(crate) fn validate_manifest(manifest: &PluginManifest) -> TeamResult<()> {
    if manifest.name.trim().is_empty() {
        return Err(opengoose_types::YamlStoreError::ValidationFailed(
            "plugin name is required".into(),
        )
        .into());
    }

    if manifest.version.trim().is_empty() {
        return Err(opengoose_types::YamlStoreError::ValidationFailed(
            "plugin version is required".into(),
        )
        .into());
    }

    if manifest.has_skill_capability() {
        for skill in &manifest.skills {
            if skill.name.trim().is_empty() {
                return Err(opengoose_types::YamlStoreError::ValidationFailed(
                    "skill name is required in [[skills]] entry".into(),
                )
                .into());
            }

            if skill.cmd.trim().is_empty() {
                return Err(opengoose_types::YamlStoreError::ValidationFailed(format!(
                    "skill '{}' requires a non-empty cmd field",
                    skill.name
                ))
                .into());
            }
        }
    }

    Ok(())
}
