use std::path::{Path, PathBuf};

use crate::error::TeamResult;

use super::manifest::PluginManifest;

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

    pub(crate) fn new(manifest: PluginManifest, path: PathBuf) -> Self {
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
