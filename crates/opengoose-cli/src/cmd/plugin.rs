use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, PluginStore};
use opengoose_teams::plugin::{
    Plugin as PluginTrait, default_plugins_dir, discover_plugins, load_manifest,
};

#[derive(Subcommand)]
/// Subcommands for `opengoose plugin`.
pub enum PluginAction {
    /// Install a plugin from a local path
    Install {
        /// Path to the plugin directory (must contain plugin.toml)
        path: PathBuf,
    },
    /// List all installed plugins
    List,
    /// Remove an installed plugin by name
    Remove {
        /// Plugin name
        name: String,
    },
    /// Show information about a plugin
    Info {
        /// Plugin name
        name: String,
    },
    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },
    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },
    /// Scan the plugins directory and show discovered (not yet installed) plugins
    Discover,
}

/// Dispatch and execute the selected plugin subcommand.
pub fn execute(action: PluginAction) -> Result<()> {
    match action {
        PluginAction::Install { path } => cmd_install(path),
        PluginAction::List => cmd_list(),
        PluginAction::Remove { name } => cmd_remove(&name),
        PluginAction::Info { name } => cmd_info(&name),
        PluginAction::Enable { name } => cmd_enable(&name),
        PluginAction::Disable { name } => cmd_disable(&name),
        PluginAction::Discover => cmd_discover(),
    }
}

fn cmd_install(path: PathBuf) -> Result<()> {
    let path = path.canonicalize().map_err(|_| {
        anyhow::anyhow!(
            "plugin path '{}' does not exist or is not accessible",
            path.display()
        )
    })?;

    if !path.is_dir() {
        bail!(
            "'{}' is not a directory. A plugin must be a directory containing plugin.toml.",
            path.display()
        );
    }

    let manifest_path = path.join("plugin.toml");
    let manifest = load_manifest(&manifest_path).map_err(|e| anyhow::anyhow!("{e}"))?;

    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    // Check if already installed
    if store.get_by_name(&manifest.name)?.is_some() {
        bail!(
            "plugin '{}' is already installed. Remove it first with `opengoose plugin remove {}`.",
            manifest.name,
            manifest.name
        );
    }

    let plugin = store.install(
        &manifest.name,
        &manifest.version,
        &path.to_string_lossy(),
        manifest.author.as_deref(),
        manifest.description.as_deref(),
        &manifest.capabilities_str(),
    )?;

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

fn cmd_list() -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);
    let plugins = store.list()?;

    if plugins.is_empty() {
        println!("No plugins installed. Use `opengoose plugin install <path>` to install one.");
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {:<10} {:<25}",
        "NAME", "VERSION", "ENABLED", "CAPABILITIES"
    );
    for p in &plugins {
        let enabled = if p.enabled { "yes" } else { "no" };
        let caps = if p.capabilities.is_empty() {
            "-".to_string()
        } else {
            p.capabilities.clone()
        };
        println!(
            "{:<25} {:<10} {:<10} {:<25}",
            p.name, p.version, enabled, caps
        );
    }

    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    if store.uninstall(name)? {
        println!("Removed plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_info(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    let plugin = store
        .get_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("plugin '{name}' not found"))?;

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

fn cmd_enable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    if store.set_enabled(name, true)? {
        println!("Enabled plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    if store.set_enabled(name, false)? {
        println!("Disabled plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_discover() -> Result<()> {
    let plugins_dir = default_plugins_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;

    println!("Scanning '{}'...", plugins_dir.display());

    let discovered = discover_plugins(&plugins_dir).map_err(|e| anyhow::anyhow!("{e}"))?;

    if discovered.is_empty() {
        println!("No plugins found.");
        println!(
            "Place plugin directories with a plugin.toml manifest under '{}'.",
            plugins_dir.display()
        );
        return Ok(());
    }

    // Cross-reference with installed plugins
    let db = Arc::new(Database::open()?);
    let store = PluginStore::new(db);

    println!(
        "{:<25} {:<10} {:<10} {:<10}",
        "NAME", "VERSION", "INSTALLED", "CAPABILITIES"
    );
    for p in &discovered {
        let installed = store.get_by_name(p.name())?.is_some();
        let installed_str = if installed { "yes" } else { "no" };
        let caps = if p.capabilities().is_empty() {
            "-"
        } else {
            p.capabilities()
        };
        println!(
            "{:<25} {:<10} {:<10} {:<10}",
            p.name(),
            p.version(),
            installed_str,
            caps
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> PluginStore {
        let db = Arc::new(Database::open_in_memory().unwrap());
        PluginStore::new(db)
    }

    // ---- PluginStore with in-memory DB ----

    #[test]
    fn plugin_store_list_empty_initially() {
        let store = make_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn plugin_store_install_and_list() {
        let store = make_store();
        let plugin = store
            .install("my-plugin", "1.0.0", "/tmp/my-plugin", None, None, "")
            .unwrap();
        assert_eq!(plugin.name, "my-plugin");
        assert_eq!(plugin.version, "1.0.0");
        assert_eq!(plugin.source_path, "/tmp/my-plugin");
        assert!(plugin.enabled);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn plugin_store_install_with_metadata() {
        let store = make_store();
        let plugin = store
            .install(
                "advanced-plugin",
                "2.1.0",
                "/tmp/advanced",
                Some("Alice"),
                Some("Does advanced things"),
                "code,chat",
            )
            .unwrap();
        assert_eq!(plugin.author.as_deref(), Some("Alice"));
        assert_eq!(plugin.description.as_deref(), Some("Does advanced things"));
        assert_eq!(plugin.capabilities, "code,chat");
    }

    #[test]
    fn plugin_store_get_by_name_returns_correct_plugin() {
        let store = make_store();
        store
            .install("plugin-a", "1.0.0", "/tmp/a", None, None, "")
            .unwrap();
        store
            .install("plugin-b", "2.0.0", "/tmp/b", None, None, "")
            .unwrap();

        let found = store.get_by_name("plugin-a").unwrap().unwrap();
        assert_eq!(found.name, "plugin-a");
        assert_eq!(found.version, "1.0.0");
    }

    #[test]
    fn plugin_store_get_by_name_returns_none_for_missing() {
        let store = make_store();
        assert!(store.get_by_name("nonexistent").unwrap().is_none());
    }

    #[test]
    fn plugin_store_uninstall_existing_returns_true() {
        let store = make_store();
        store
            .install("to-remove", "1.0.0", "/tmp/remove", None, None, "")
            .unwrap();
        assert!(store.uninstall("to-remove").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn plugin_store_uninstall_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.uninstall("ghost").unwrap());
    }

    #[test]
    fn plugin_store_set_enabled_disable() {
        let store = make_store();
        store
            .install("toggle-plugin", "1.0.0", "/tmp/toggle", None, None, "")
            .unwrap();

        assert!(store.set_enabled("toggle-plugin", false).unwrap());
        let p = store.get_by_name("toggle-plugin").unwrap().unwrap();
        assert!(!p.enabled);
    }

    #[test]
    fn plugin_store_set_enabled_re_enable() {
        let store = make_store();
        store
            .install("toggle-plugin2", "1.0.0", "/tmp/toggle2", None, None, "")
            .unwrap();

        store.set_enabled("toggle-plugin2", false).unwrap();
        assert!(store.set_enabled("toggle-plugin2", true).unwrap());
        let p = store.get_by_name("toggle-plugin2").unwrap().unwrap();
        assert!(p.enabled);
    }

    #[test]
    fn plugin_store_set_enabled_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.set_enabled("nonexistent", true).unwrap());
    }

    #[test]
    fn plugin_store_list_enabled_filters_disabled() {
        let store = make_store();
        store
            .install("enabled-plugin", "1.0.0", "/tmp/e", None, None, "")
            .unwrap();
        store
            .install("disabled-plugin", "1.0.0", "/tmp/d", None, None, "")
            .unwrap();
        store.set_enabled("disabled-plugin", false).unwrap();

        let enabled = store.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "enabled-plugin");
    }

    #[test]
    fn plugin_store_capabilities_stored_correctly() {
        let store = make_store();
        store
            .install("cap-plugin", "1.0.0", "/tmp/cap", None, None, "code,search")
            .unwrap();

        let p = store.get_by_name("cap-plugin").unwrap().unwrap();
        let caps = p.capability_list();
        assert_eq!(caps, vec!["code", "search"]);
    }

    #[test]
    fn plugin_store_empty_capabilities_list() {
        let store = make_store();
        store
            .install("nocap-plugin", "1.0.0", "/tmp/nocap", None, None, "")
            .unwrap();

        let p = store.get_by_name("nocap-plugin").unwrap().unwrap();
        assert!(p.capability_list().is_empty());
    }
}
