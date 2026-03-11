use std::path::{Path, PathBuf};

use crate::error::TeamResult;

use super::{
    loaded::{LoadedPlugin, Plugin},
    manifest::{PluginManifest, validate_manifest},
};

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
            Ok(manifest) => plugins.push(LoadedPlugin::new(manifest, path)),
            Err(error) => {
                tracing::warn!(
                    path = %manifest_path.display(),
                    error = %error,
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
    let manifest: PluginManifest = toml::from_str(&content).map_err(|error| {
        opengoose_types::YamlStoreError::ValidationFailed(format!(
            "invalid plugin.toml at {}: {error}",
            path.display()
        ))
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Default plugins directory: `~/.opengoose/plugins/`.
pub fn default_plugins_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".opengoose").join("plugins"))
}
