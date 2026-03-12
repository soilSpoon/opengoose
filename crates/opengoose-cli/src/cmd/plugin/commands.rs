use std::path::PathBuf;
use std::sync::Arc;

use crate::error::{CliError, CliResult};

use opengoose_core::plugins::{
    PluginInstallOutcome, PluginRemoveOutcome, install_plugin, remove_plugin, set_plugin_enabled,
};
use opengoose_persistence::{Database, PluginStore};
use opengoose_teams::plugin::{Plugin as PluginTrait, default_plugins_dir, discover_plugins};

pub(super) fn install(db: Arc<Database>, path: PathBuf) -> CliResult<()> {
    let PluginInstallOutcome {
        plugin,
        registered_skills,
    } = install_plugin(db, path)?;

    if !registered_skills.is_empty() {
        println!(
            "Registered {} skill(s): {}",
            registered_skills.len(),
            registered_skills.join(", ")
        );
    }
    println!("Installed plugin '{}'.", plugin.name);
    println!("  Version: {}", plugin.version);
    if let Some(ref desc) = plugin.description {
        println!("  Description: {desc}");
    }
    if !plugin.capabilities.is_empty() {
        println!("  Capabilities: {}", plugin.capabilities);
    }
    println!("  Path: {}", plugin.source_path);

    Ok(())
}

pub(super) fn list(store: &PluginStore) -> CliResult<()> {
    let plugins = store.list()?;

    if plugins.is_empty() {
        println!("No plugins installed. Use `opengoose plugin install <path>` to install one.");
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {:<10} {:<25}",
        "NAME", "VERSION", "ENABLED", "CAPABILITIES"
    );
    for plugin in &plugins {
        let enabled = if plugin.enabled { "yes" } else { "no" };
        let capabilities = if plugin.capabilities.is_empty() {
            "-".to_string()
        } else {
            plugin.capabilities.clone()
        };
        println!(
            "{:<25} {:<10} {:<10} {:<25}",
            plugin.name, plugin.version, enabled, capabilities
        );
    }

    Ok(())
}

pub(super) fn remove(db: Arc<Database>, name: &str) -> CliResult<()> {
    let PluginRemoveOutcome {
        removed,
        removed_skills,
    } = remove_plugin(db, name)?;

    if !removed_skills.is_empty() {
        println!(
            "Removed {} skill(s): {}",
            removed_skills.len(),
            removed_skills.join(", ")
        );
    }

    if removed {
        println!("Removed plugin '{name}'.");
    } else {
        return Err(CliError::Validation(format!("plugin '{name}' not found.")));
    }

    Ok(())
}

pub(super) fn info(store: &PluginStore, name: &str) -> CliResult<()> {
    let plugin = store
        .get_by_name(name)?
        .ok_or_else(|| CliError::Validation(format!("plugin '{name}' not found")))?;

    println!("Plugin: {}", plugin.name);
    println!("  Version: {}", plugin.version);
    println!("  Enabled: {}", if plugin.enabled { "yes" } else { "no" });
    if let Some(ref author) = plugin.author {
        println!("  Author: {author}");
    }
    if let Some(ref desc) = plugin.description {
        println!("  Description: {desc}");
    }
    if !plugin.capabilities.is_empty() {
        println!("  Capabilities: {}", plugin.capabilities);
    }
    println!("  Path: {}", plugin.source_path);
    println!("  Installed: {}", plugin.created_at);
    println!("  Updated: {}", plugin.updated_at);

    Ok(())
}

pub(super) fn enable(db: Arc<Database>, name: &str) -> CliResult<()> {
    set_enabled(db, name, true, "Enabled")
}

pub(super) fn disable(db: Arc<Database>, name: &str) -> CliResult<()> {
    set_enabled(db, name, false, "Disabled")
}

pub(super) fn discover(store: &PluginStore) -> CliResult<()> {
    let plugins_dir = default_plugins_dir()
        .ok_or_else(|| CliError::Validation(format!("could not determine home directory")))?;

    println!("Scanning '{}'...", plugins_dir.display());

    let discovered = discover_plugins(&plugins_dir).map_err(|e| CliError::Validation(format!("{e}")))?;

    if discovered.is_empty() {
        println!("No plugins found.");
        println!(
            "Place plugin directories with a plugin.toml manifest under '{}'.",
            plugins_dir.display()
        );
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {:<10} {:<10}",
        "NAME", "VERSION", "INSTALLED", "CAPABILITIES"
    );
    for plugin in &discovered {
        let installed = store.get_by_name(plugin.name())?.is_some();
        let installed_str = if installed { "yes" } else { "no" };
        let capabilities = if plugin.capabilities().is_empty() {
            "-"
        } else {
            plugin.capabilities()
        };
        println!(
            "{:<25} {:<10} {:<10} {:<10}",
            plugin.name(),
            plugin.version(),
            installed_str,
            capabilities
        );
    }

    Ok(())
}

fn set_enabled(db: Arc<Database>, name: &str, enabled: bool, verb: &str) -> CliResult<()> {
    if set_plugin_enabled(db, name, enabled)? {
        println!("{verb} plugin '{name}'.");
        Ok(())
    } else {
        return Err(CliError::Validation(format!("plugin '{name}' not found.")));
    }
}
