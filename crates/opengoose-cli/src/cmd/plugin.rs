use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, PluginStore};
use opengoose_profiles::SkillStore;
use opengoose_teams::plugin::{
    LoadedPlugin, Plugin as PluginTrait, PluginRuntime, default_plugins_dir, discover_plugins,
    load_manifest,
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
    let db = Arc::new(Database::open()?);
    run(action, db)
}

/// Testable dispatch: accepts injected db.
pub(crate) fn run(action: PluginAction, db: Arc<Database>) -> Result<()> {
    let store = PluginStore::new(db.clone());
    match action {
        PluginAction::Install { path } => cmd_install(&store, path),
        PluginAction::List => cmd_list(&store),
        PluginAction::Remove { name } => cmd_remove(&store, &name),
        PluginAction::Info { name } => cmd_info(&store, &name),
        PluginAction::Enable { name } => cmd_enable(&store, &name),
        PluginAction::Disable { name } => cmd_disable(&store, &name),
        PluginAction::Discover => cmd_discover(&store),
    }
}

fn cmd_install(store: &PluginStore, path: PathBuf) -> Result<()> {
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
    let manifest = load_manifest(&manifest_path).with_context(|| {
        format!(
            "failed to load plugin manifest at {}",
            manifest_path.display()
        )
    })?;

    // Check if already installed
    if store.get_by_name(&manifest.name)?.is_some() {
        bail!(
            "plugin '{}' is already installed. Remove it first with `opengoose plugin remove {}`.",
            manifest.name,
            manifest.name
        );
    }

    // Register plugin skills before persisting to DB.
    let loaded = LoadedPlugin::from_manifest(manifest.clone(), path.clone());
    if let Ok(skill_store) = SkillStore::new() {
        let init_result = PluginRuntime::init_plugin(&loaded, &skill_store)
            .with_context(|| format!("failed to initialize plugin '{}'", manifest.name))?;
        if !init_result.registered_skills.is_empty() {
            println!(
                "Registered {} skill(s): {}",
                init_result.registered_skills.len(),
                init_result.registered_skills.join(", ")
            );
        }
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

fn cmd_list(store: &PluginStore) -> Result<()> {
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

fn cmd_remove(store: &PluginStore, name: &str) -> Result<()> {
    // Shutdown plugin skills before removing from DB.
    if let Some(ref record) = store.get_by_name(name)? {
        let source = std::path::Path::new(&record.source_path);
        let manifest_path = source.join("plugin.toml");
        if manifest_path.exists()
            && let Ok(manifest) = load_manifest(&manifest_path)
        {
            let loaded = LoadedPlugin::from_manifest(manifest, source.to_path_buf());
            if let Ok(skill_store) = SkillStore::new()
                && let Ok(removed) = PluginRuntime::shutdown_plugin(&loaded, &skill_store)
                && !removed.is_empty()
            {
                println!("Removed {} skill(s): {}", removed.len(), removed.join(", "));
            }
        }
    }

    if store.uninstall(name)? {
        println!("Removed plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_info(store: &PluginStore, name: &str) -> Result<()> {
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

fn cmd_enable(store: &PluginStore, name: &str) -> Result<()> {
    if store.set_enabled(name, true)? {
        println!("Enabled plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(store: &PluginStore, name: &str) -> Result<()> {
    if store.set_enabled(name, false)? {
        println!("Disabled plugin '{name}'.");
    } else {
        bail!("plugin '{name}' not found.");
    }

    Ok(())
}

fn cmd_discover(store: &PluginStore) -> Result<()> {
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

    fn make_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    /// Create a temp plugin directory with a minimal plugin.toml.
    fn make_plugin_dir(name: &str, version: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let toml = format!("name = \"{name}\"\nversion = \"{version}\"\n");
        std::fs::write(dir.path().join("plugin.toml"), toml).unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    /// Create a plugin dir with full metadata.
    fn make_plugin_dir_full(
        name: &str,
        version: &str,
        author: &str,
        description: &str,
        capabilities: &[&str],
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let caps_toml = capabilities
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let toml = format!(
            "name = \"{name}\"\nversion = \"{version}\"\nauthor = \"{author}\"\ndescription = \"{description}\"\ncapabilities = [{caps_toml}]\n"
        );
        std::fs::write(dir.path().join("plugin.toml"), toml).unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
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

    // ---- CLI dispatch path tests via run() ----

    #[test]
    fn dispatch_list_empty_succeeds() {
        let db = make_db();
        assert!(run(PluginAction::List, db).is_ok());
    }

    #[test]
    fn dispatch_install_from_valid_dir_succeeds() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir("my-skill", "1.0.0");

        let result = run(PluginAction::Install { path: plugin_path }, db.clone());
        assert!(result.is_ok(), "install should succeed: {result:?}");

        let p = PluginStore::new(db)
            .get_by_name("my-skill")
            .unwrap()
            .unwrap();
        assert_eq!(p.version, "1.0.0");
        assert!(p.enabled);
    }

    #[test]
    fn dispatch_install_with_full_metadata_succeeds() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir_full(
            "rich-plugin",
            "2.0.0",
            "Bob",
            "Does rich things",
            &["skill"],
        );

        let result = run(PluginAction::Install { path: plugin_path }, db.clone());
        assert!(result.is_ok(), "install should succeed: {result:?}");

        let p = PluginStore::new(db)
            .get_by_name("rich-plugin")
            .unwrap()
            .unwrap();
        assert_eq!(p.author.as_deref(), Some("Bob"));
        assert_eq!(p.description.as_deref(), Some("Does rich things"));
        assert!(!p.capabilities.is_empty());
    }

    #[test]
    fn dispatch_install_nonexistent_path_errors() {
        let db = make_db();
        let result = run(
            PluginAction::Install {
                path: PathBuf::from("/nonexistent/path/to/plugin"),
            },
            db,
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist") || err.contains("not accessible"));
    }

    #[test]
    fn dispatch_install_duplicate_errors() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir("dup-plugin", "1.0.0");

        // First install succeeds
        run(
            PluginAction::Install {
                path: plugin_path.clone(),
            },
            db.clone(),
        )
        .unwrap();

        // Second install should fail with "already installed"
        let result = run(PluginAction::Install { path: plugin_path }, db);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already installed")
        );
    }

    #[test]
    fn dispatch_list_shows_installed_plugin() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir("listed-plugin", "1.0.0");

        run(PluginAction::Install { path: plugin_path }, db.clone()).unwrap();

        let result = run(PluginAction::List, db);
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_remove_installed_plugin_succeeds() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir("removable", "1.0.0");

        run(PluginAction::Install { path: plugin_path }, db.clone()).unwrap();

        let result = run(
            PluginAction::Remove {
                name: "removable".to_string(),
            },
            db.clone(),
        );
        assert!(result.is_ok());

        assert!(
            PluginStore::new(db)
                .get_by_name("removable")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn dispatch_remove_nonexistent_plugin_errors() {
        let db = make_db();
        let result = run(
            PluginAction::Remove {
                name: "ghost".to_string(),
            },
            db,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_info_installed_plugin_succeeds() {
        let db = make_db();
        PluginStore::new(db.clone())
            .install("info-plugin", "3.0.0", "/tmp/info", Some("Dev"), None, "")
            .unwrap();

        let result = run(
            PluginAction::Info {
                name: "info-plugin".to_string(),
            },
            db,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_info_nonexistent_errors() {
        let db = make_db();
        let result = run(
            PluginAction::Info {
                name: "no-plugin".to_string(),
            },
            db,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_enable_plugin_succeeds() {
        let db = make_db();
        let store = PluginStore::new(db.clone());
        store
            .install("enableable", "1.0.0", "/tmp/e", None, None, "")
            .unwrap();
        store.set_enabled("enableable", false).unwrap();

        let result = run(
            PluginAction::Enable {
                name: "enableable".to_string(),
            },
            db.clone(),
        );
        assert!(result.is_ok());

        let p = PluginStore::new(db)
            .get_by_name("enableable")
            .unwrap()
            .unwrap();
        assert!(p.enabled);
    }

    #[test]
    fn dispatch_enable_nonexistent_errors() {
        let db = make_db();
        let result = run(
            PluginAction::Enable {
                name: "no-plugin".to_string(),
            },
            db,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_disable_plugin_succeeds() {
        let db = make_db();
        PluginStore::new(db.clone())
            .install("disableable", "1.0.0", "/tmp/d", None, None, "")
            .unwrap();

        let result = run(
            PluginAction::Disable {
                name: "disableable".to_string(),
            },
            db.clone(),
        );
        assert!(result.is_ok());

        let p = PluginStore::new(db)
            .get_by_name("disableable")
            .unwrap()
            .unwrap();
        assert!(!p.enabled);
    }

    #[test]
    fn dispatch_disable_nonexistent_errors() {
        let db = make_db();
        let result = run(
            PluginAction::Disable {
                name: "no-plugin".to_string(),
            },
            db,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_install_list_remove_lifecycle() {
        let db = make_db();
        let (_dir, plugin_path) = make_plugin_dir("lifecycle-plugin", "1.0.0");

        run(PluginAction::Install { path: plugin_path }, db.clone()).unwrap();
        run(PluginAction::List, db.clone()).unwrap();
        run(
            PluginAction::Remove {
                name: "lifecycle-plugin".to_string(),
            },
            db.clone(),
        )
        .unwrap();

        assert!(PluginStore::new(db).list().unwrap().is_empty());
    }

    #[test]
    fn dispatch_enable_disable_toggle_plugin() {
        let db = make_db();
        PluginStore::new(db.clone())
            .install("toggle-dispatch", "1.0.0", "/tmp/t", None, None, "")
            .unwrap();

        run(
            PluginAction::Disable {
                name: "toggle-dispatch".to_string(),
            },
            db.clone(),
        )
        .unwrap();

        let p = PluginStore::new(db.clone())
            .get_by_name("toggle-dispatch")
            .unwrap()
            .unwrap();
        assert!(!p.enabled);

        run(
            PluginAction::Enable {
                name: "toggle-dispatch".to_string(),
            },
            db.clone(),
        )
        .unwrap();

        let p = PluginStore::new(db)
            .get_by_name("toggle-dispatch")
            .unwrap()
            .unwrap();
        assert!(p.enabled);
    }
}
