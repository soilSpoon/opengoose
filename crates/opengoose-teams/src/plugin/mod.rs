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

mod discovery;
mod loaded;
mod manifest;
mod runtime;

#[cfg(test)]
mod tests;

pub use discovery::{default_plugins_dir, discover_plugins, load_manifest};
pub use loaded::{LoadedPlugin, Plugin};
pub use manifest::{PluginManifest, PluginSkillDef};
pub use runtime::{PluginInitResult, PluginRuntime};
