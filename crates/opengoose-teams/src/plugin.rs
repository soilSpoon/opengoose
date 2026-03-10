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
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::TeamResult;

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
}

impl PluginManifest {
    /// Capabilities joined as a comma-separated string for persistence.
    pub fn capabilities_str(&self) -> String {
        self.capabilities.join(", ")
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
}

/// A plugin loaded from a `plugin.toml` manifest on disk.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    manifest: PluginManifest,
    path: PathBuf,
    capabilities_str: String,
}

impl LoadedPlugin {
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
    }

    #[test]
    fn test_validate_rejects_empty_name() {
        let m = PluginManifest {
            name: "  ".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec![],
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
        };
        assert!(validate_manifest(&m).is_err());
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
    }
}
