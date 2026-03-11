//! Plugin system for dynamic skill loading and channel adapter registration.
//!
//! This module defines the `Plugin` trait that all OpenGoose plugins must implement,
//! along with a filesystem-based plugin loader that discovers plugins from
//! `~/.opengoose/plugins/`.
//!
//! # Plugin discovery
//! Plugins are directories under `~/.opengoose/plugins/` that contain a
//! `plugin.toml` manifest file describing the plugin metadata.
//!
//! # Example manifest (`plugin.toml`)
//! ```toml
//! name = "my-skill"
//! version = "1.0.0"
//! author = "Jane Doe"
//! description = "Adds custom shell tools as skills"
//! capabilities = ["skill"]
//!
//! [[skills]]
//! name = "git-log"
//! cmd = "git"
//! args = ["log", "--oneline", "-20"]
//! description = "Show recent commits"
//! ```

use std::path::{Path, PathBuf};

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
    pub envs: std::collections::HashMap<String, String>,
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

/// Core trait that every OpenGoose plugin must implement.
///
/// For the initial filesystem-based implementation, plugins are represented
/// as a `LoadedPlugin` struct parsed from a `plugin.toml` manifest.
/// Future versions may support shared-library or WASM plugins by implementing
/// this trait with dynamic dispatch.
pub trait Plugin: Send + Sync {
    /// Human-readable plugin name (matches `plugin.toml` name field).
    fn name(&self) -> &str;
    /// SemVer version string.
    fn version(&self) -> &str;
    /// Comma-separated capability tags.
    fn capabilities(&self) -> &str;
    /// Path to the plugin on disk.
    fn source_path(&self) -> &Path;

    /// Initialise the plugin. Called once after loading.
    ///
    /// For filesystem plugins this is a no-op; dynamic (.so/WASM) plugins
    /// may perform registration here.
    fn init(&self) -> TeamResult<()> {
        Ok(())
    }

    /// Shut down the plugin. Called before removal or application exit.
    fn shutdown(&self) -> TeamResult<()> {
        Ok(())
    }
}

/// A plugin loaded from a `plugin.toml` manifest on disk.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    manifest: PluginManifest,
    path: PathBuf,
    capabilities_str: String,
}

impl LoadedPlugin {
    /// Create a `LoadedPlugin` from a parsed manifest and its directory path.
    pub fn from_manifest(manifest: PluginManifest, path: PathBuf) -> Self {
        Self::new(manifest, path)
    }

    fn new(manifest: PluginManifest, path: PathBuf) -> Self {
        let capabilities_str = manifest.capabilities_str();
        Self {
            manifest,
            path,
            capabilities_str,
        }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

impl Plugin for LoadedPlugin {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn version(&self) -> &str {
        &self.manifest.version
    }

    fn capabilities(&self) -> &str {
        &self.capabilities_str
    }

    fn source_path(&self) -> &Path {
        &self.path
    }
}

/// Plugin lifecycle manager that handles skill registration and cleanup.
///
/// `PluginRuntime` bridges the gap between the plugin manifest (what a plugin
/// declares) and the skill store (where registered skills live). It converts
/// `PluginSkillDef` entries from a manifest into `Skill` objects and persists
/// them via the `SkillStore`.
pub struct PluginRuntime;

impl PluginRuntime {
    /// Initialise a plugin: register its skill definitions with the skill store.
    ///
    /// For plugins with the `skill` capability, each `[[skills]]` entry in the
    /// manifest is converted to a `Skill` and saved to the store. Skill names
    /// are prefixed with the plugin name to avoid collisions (e.g.
    /// `my-plugin/git-log`).
    ///
    /// For plugins with the `channel_adapter` capability, a log message is
    /// emitted. Dynamic channel adapter loading is not yet implemented.
    pub fn init_plugin(
        plugin: &LoadedPlugin,
        skill_store: &opengoose_profiles::SkillStore,
    ) -> TeamResult<PluginInitResult> {
        let manifest = plugin.manifest();
        let mut registered_skills = Vec::new();

        if manifest.has_skill_capability() {
            for skill_def in &manifest.skills {
                let skill_name = format!("{}/{}", manifest.name, skill_def.name);
                let ext = opengoose_profiles::ExtensionRef {
                    name: skill_def.name.clone(),
                    ext_type: "stdio".to_string(),
                    cmd: Some(skill_def.cmd.clone()),
                    args: skill_def.args.clone(),
                    uri: None,
                    timeout: skill_def.timeout,
                    envs: skill_def.envs.clone(),
                    env_keys: vec![],
                    code: None,
                    dependencies: None,
                };

                let skill = opengoose_profiles::Skill {
                    name: skill_name.clone(),
                    description: skill_def.description.clone(),
                    version: manifest.version.clone(),
                    extensions: vec![ext],
                };

                // Overwrite if already registered (plugin update path).
                skill_store.save(&skill, true).map_err(|e| {
                    crate::error::TeamError::PluginInit(format!(
                        "failed to register skill '{}': {}",
                        skill_name, e
                    ))
                })?;

                tracing::info!(
                    plugin = %manifest.name,
                    skill = %skill_name,
                    cmd = %skill_def.cmd,
                    "registered plugin skill"
                );
                registered_skills.push(skill_name);
            }
        }

        if manifest.has_channel_adapter_capability() {
            tracing::info!(
                plugin = %manifest.name,
                "plugin declares channel_adapter capability (dynamic loading not yet supported)"
            );
        }

        Ok(PluginInitResult {
            plugin_name: manifest.name.clone(),
            registered_skills,
        })
    }

    /// Shut down a plugin: remove its registered skills from the skill store.
    ///
    /// Removes all skills prefixed with `{plugin_name}/` from the store.
    pub fn shutdown_plugin(
        plugin: &LoadedPlugin,
        skill_store: &opengoose_profiles::SkillStore,
    ) -> TeamResult<Vec<String>> {
        let manifest = plugin.manifest();
        let mut removed = Vec::new();

        if manifest.has_skill_capability() {
            for skill_def in &manifest.skills {
                let skill_name = format!("{}/{}", manifest.name, skill_def.name);
                match skill_store.remove(&skill_name) {
                    Ok(()) => {
                        tracing::info!(
                            plugin = %manifest.name,
                            skill = %skill_name,
                            "removed plugin skill"
                        );
                        removed.push(skill_name);
                    }
                    Err(opengoose_profiles::ProfileError::SkillNotFound(_)) => {
                        tracing::debug!(
                            plugin = %manifest.name,
                            skill = %skill_name,
                            "skill already removed or never registered"
                        );
                    }
                    Err(e) => {
                        return Err(crate::error::TeamError::PluginInit(format!(
                            "failed to remove skill '{}': {}",
                            skill_name, e
                        )));
                    }
                }
            }
        }

        Ok(removed)
    }
}

/// Result of initializing a plugin.
#[derive(Debug)]
pub struct PluginInitResult {
    /// Name of the plugin that was initialized.
    pub plugin_name: String,
    /// Names of skills that were registered.
    pub registered_skills: Vec<String>,
}

/// Discover and load plugins from a directory.
///
/// Each direct subdirectory that contains a `plugin.toml` file is treated
/// as a plugin. Returns a `LoadedPlugin` for each valid manifest found.
pub fn discover_plugins(plugins_dir: &Path) -> TeamResult<Vec<LoadedPlugin>> {
    let mut plugins = Vec::new();

    if !plugins_dir.exists() {
        return Ok(plugins);
    }

    let entries = std::fs::read_dir(plugins_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.toml");
        if !manifest_path.exists() {
            continue;
        }

        match load_manifest(&manifest_path) {
            Ok(manifest) => {
                plugins.push(LoadedPlugin::new(manifest, path));
            }
            Err(e) => {
                tracing::warn!(
                    path = %manifest_path.display(),
                    error = %e,
                    "skipping plugin with invalid manifest"
                );
            }
        }
    }

    plugins.sort_by(|a, b| a.name().cmp(b.name()));
    Ok(plugins)
}

/// Load a `PluginManifest` from a `plugin.toml` path.
pub fn load_manifest(path: &Path) -> TeamResult<PluginManifest> {
    let content = std::fs::read_to_string(path)?;
    let manifest: PluginManifest = toml::from_str(&content).map_err(|e| {
        opengoose_types::YamlStoreError::ValidationFailed(format!(
            "invalid plugin.toml at {}: {e}",
            path.display()
        ))
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(m: &PluginManifest) -> TeamResult<()> {
    if m.name.trim().is_empty() {
        return Err(opengoose_types::YamlStoreError::ValidationFailed(
            "plugin name is required".into(),
        )
        .into());
    }
    if m.version.trim().is_empty() {
        return Err(opengoose_types::YamlStoreError::ValidationFailed(
            "plugin version is required".into(),
        )
        .into());
    }
    // Validate skill definitions if skill capability is declared.
    if m.has_skill_capability() {
        for skill in &m.skills {
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

/// Default plugins directory: `~/.opengoose/plugins/`.
pub fn default_plugins_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".opengoose").join("plugins"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(dir: &Path, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), content).unwrap();
    }

    #[test]
    fn test_manifest_parse() {
        let toml_str = r#"
name = "git-skill"
version = "1.2.3"
author = "Bob"
description = "Git tools"
capabilities = ["skill"]
"#;
        let m: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.name, "git-skill");
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.author.as_deref(), Some("Bob"));
        assert_eq!(m.capabilities, vec!["skill"]);
        assert_eq!(m.capabilities_str(), "skill");
    }

    #[test]
    fn test_manifest_minimal() {
        let toml_str = r#"
name = "minimal"
version = "0.1.0"
"#;
        let m: PluginManifest = toml::from_str(toml_str).unwrap();
        assert!(m.author.is_none());
        assert!(m.capabilities.is_empty());
        assert_eq!(m.capabilities_str(), "");
        assert!(m.skills.is_empty());
    }

    #[test]
    fn test_manifest_with_skills() {
        let toml_str = r#"
name = "git-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "git-log"
cmd = "git"
args = ["log", "--oneline", "-20"]
description = "Show recent commits"
timeout = 30

[[skills]]
name = "git-status"
cmd = "git"
args = ["status"]
"#;
        let m: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.skills.len(), 2);
        assert_eq!(m.skills[0].name, "git-log");
        assert_eq!(m.skills[0].cmd, "git");
        assert_eq!(m.skills[0].args, vec!["log", "--oneline", "-20"]);
        assert_eq!(
            m.skills[0].description.as_deref(),
            Some("Show recent commits")
        );
        assert_eq!(m.skills[0].timeout, Some(30));
        assert_eq!(m.skills[1].name, "git-status");
        assert!(m.skills[1].description.is_none());
        assert_eq!(m.skills[1].timeout, None);
    }

    #[test]
    fn test_manifest_skill_with_envs() {
        let toml_str = r#"
name = "env-plugin"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "custom-tool"
cmd = "my-tool"
envs = { API_KEY = "test", MODE = "production" }
"#;
        let m: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.skills[0].envs.len(), 2);
        assert_eq!(m.skills[0].envs.get("API_KEY").unwrap(), "test");
        assert_eq!(m.skills[0].envs.get("MODE").unwrap(), "production");
    }

    #[test]
    fn test_validate_rejects_empty_name() {
        let m = PluginManifest {
            name: "  ".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec![],
            skills: vec![],
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_version() {
        let m = PluginManifest {
            name: "ok".into(),
            version: "".into(),
            author: None,
            description: None,
            capabilities: vec![],
            skills: vec![],
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_skill_name() {
        let m = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec!["skill".into()],
            skills: vec![PluginSkillDef {
                name: "  ".into(),
                cmd: "echo".into(),
                args: vec![],
                description: None,
                timeout: None,
                envs: Default::default(),
            }],
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_skill_cmd() {
        let m = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec!["skill".into()],
            skills: vec![PluginSkillDef {
                name: "my-skill".into(),
                cmd: "".into(),
                args: vec![],
                description: None,
                timeout: None,
                envs: Default::default(),
            }],
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn test_has_skill_capability() {
        let m = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec!["skill".into(), "other".into()],
            skills: vec![],
        };
        assert!(m.has_skill_capability());
        assert!(!m.has_channel_adapter_capability());
    }

    #[test]
    fn test_has_channel_adapter_capability() {
        let m = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec!["channel_adapter".into()],
            skills: vec![],
        };
        assert!(!m.has_skill_capability());
        assert!(m.has_channel_adapter_capability());
    }

    #[test]
    fn test_discover_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_root = tmp.path().join("plugins");

        let plugin_a = plugins_root.join("plugin-a");
        write_manifest(
            &plugin_a,
            "name = \"plugin-a\"\nversion = \"1.0.0\"\ncapabilities = [\"skill\"]\n",
        );

        let plugin_b = plugins_root.join("plugin-b");
        write_manifest(&plugin_b, "name = \"plugin-b\"\nversion = \"2.0.0\"\n");

        // A directory without plugin.toml should be skipped
        std::fs::create_dir_all(plugins_root.join("not-a-plugin")).unwrap();

        let discovered = discover_plugins(&plugins_root).unwrap();
        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].name(), "plugin-a");
        assert_eq!(discovered[1].name(), "plugin-b");
    }

    #[test]
    fn test_discover_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_root = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins_root).unwrap();

        let discovered = discover_plugins(&plugins_root).unwrap();
        assert!(discovered.is_empty());
    }

    #[test]
    fn test_discover_nonexistent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_root = tmp.path().join("does-not-exist");

        let discovered = discover_plugins(&plugins_root).unwrap();
        assert!(discovered.is_empty());
    }

    #[test]
    fn test_loaded_plugin_trait() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join("my-plugin");
        write_manifest(
            &plugin_dir,
            "name = \"my-plugin\"\nversion = \"1.0.0\"\ncapabilities = [\"skill\", \"channel_adapter\"]\n",
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir.clone());

        assert_eq!(loaded.name(), "my-plugin");
        assert_eq!(loaded.version(), "1.0.0");
        assert_eq!(loaded.capabilities(), "skill, channel_adapter");
        assert_eq!(loaded.source_path(), plugin_dir.as_path());
        assert!(loaded.init().is_ok());
        assert!(loaded.shutdown().is_ok());
    }

    #[test]
    fn test_plugin_runtime_init_registers_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("my-plugin");
        write_manifest(
            &plugin_dir,
            r#"
name = "git-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "git-log"
cmd = "git"
args = ["log", "--oneline"]
description = "Recent commits"
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir);

        let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
        assert_eq!(result.plugin_name, "git-tools");
        assert_eq!(result.registered_skills, vec!["git-tools/git-log"]);

        // Verify skill was persisted.
        let skill = skill_store.get("git-tools/git-log").unwrap();
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(skill.extensions.len(), 1);
        assert_eq!(skill.extensions[0].name, "git-log");
        assert_eq!(skill.extensions[0].cmd.as_deref(), Some("git"));
        assert_eq!(skill.extensions[0].args, vec!["log", "--oneline"]);
    }

    #[test]
    fn test_plugin_runtime_init_multiple_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("multi");
        write_manifest(
            &plugin_dir,
            r#"
name = "multi-tool"
version = "2.0.0"
capabilities = ["skill"]

[[skills]]
name = "tool-a"
cmd = "echo"
args = ["a"]

[[skills]]
name = "tool-b"
cmd = "echo"
args = ["b"]
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir);

        let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
        assert_eq!(result.registered_skills.len(), 2);
        assert_eq!(result.registered_skills[0], "multi-tool/tool-a");
        assert_eq!(result.registered_skills[1], "multi-tool/tool-b");

        assert!(skill_store.get("multi-tool/tool-a").is_ok());
        assert!(skill_store.get("multi-tool/tool-b").is_ok());
    }

    #[test]
    fn test_plugin_runtime_init_no_skills_capability() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("adapter");
        write_manifest(
            &plugin_dir,
            r#"
name = "my-adapter"
version = "1.0.0"
capabilities = ["channel_adapter"]
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir);

        let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
        assert!(result.registered_skills.is_empty());
    }

    #[test]
    fn test_plugin_runtime_shutdown_removes_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("removable");
        write_manifest(
            &plugin_dir,
            r#"
name = "removable"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "tool-x"
cmd = "echo"
args = ["x"]
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir);

        // Init first to register the skill.
        PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
        assert!(skill_store.get("removable/tool-x").is_ok());

        // Shutdown should remove it.
        let removed = PluginRuntime::shutdown_plugin(&loaded, &skill_store).unwrap();
        assert_eq!(removed, vec!["removable/tool-x"]);
        assert!(skill_store.get("removable/tool-x").is_err());
    }

    #[test]
    fn test_plugin_runtime_shutdown_nonexistent_skill_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("ghost");
        write_manifest(
            &plugin_dir,
            r#"
name = "ghost-plugin"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "phantom"
cmd = "echo"
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir);

        // Shutdown without prior init should succeed (skills not found is OK).
        let removed = PluginRuntime::shutdown_plugin(&loaded, &skill_store).unwrap();
        assert!(removed.is_empty());
    }

    #[test]
    fn test_plugin_runtime_init_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills");
        let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

        let plugin_dir = tmp.path().join("updatable");
        write_manifest(
            &plugin_dir,
            r#"
name = "updatable"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "my-tool"
cmd = "echo"
args = ["v1"]
"#,
        );

        let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded = LoadedPlugin::new(manifest, plugin_dir.clone());
        PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();

        // Update the manifest and re-init.
        write_manifest(
            &plugin_dir,
            r#"
name = "updatable"
version = "2.0.0"
capabilities = ["skill"]

[[skills]]
name = "my-tool"
cmd = "echo"
args = ["v2"]
"#,
        );

        let manifest2 = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
        let loaded2 = LoadedPlugin::new(manifest2, plugin_dir);
        let result = PluginRuntime::init_plugin(&loaded2, &skill_store).unwrap();
        assert_eq!(result.registered_skills, vec!["updatable/my-tool"]);

        let skill = skill_store.get("updatable/my-tool").unwrap();
        assert_eq!(skill.version, "2.0.0");
        assert_eq!(skill.extensions[0].args, vec!["v2"]);
    }
}
