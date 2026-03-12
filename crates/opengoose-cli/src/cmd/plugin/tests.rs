use std::path::PathBuf;
use std::sync::Arc;

use opengoose_persistence::{Database, PluginStore};

use super::{PluginAction, run};

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
        .map(|capability| format!("\"{capability}\""))
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
    let plugin = store.get_by_name("toggle-plugin").unwrap().unwrap();
    assert!(!plugin.enabled);
}

#[test]
fn plugin_store_set_enabled_re_enable() {
    let store = make_store();
    store
        .install("toggle-plugin2", "1.0.0", "/tmp/toggle2", None, None, "")
        .unwrap();

    store.set_enabled("toggle-plugin2", false).unwrap();
    assert!(store.set_enabled("toggle-plugin2", true).unwrap());
    let plugin = store.get_by_name("toggle-plugin2").unwrap().unwrap();
    assert!(plugin.enabled);
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

    let plugin = store.get_by_name("cap-plugin").unwrap().unwrap();
    let capabilities = plugin.capability_list();
    assert_eq!(capabilities, vec!["code", "search"]);
}

#[test]
fn plugin_store_empty_capabilities_list() {
    let store = make_store();
    store
        .install("nocap-plugin", "1.0.0", "/tmp/nocap", None, None, "")
        .unwrap();

    let plugin = store.get_by_name("nocap-plugin").unwrap().unwrap();
    assert!(plugin.capability_list().is_empty());
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

    let plugin = PluginStore::new(db)
        .get_by_name("my-skill")
        .unwrap()
        .unwrap();
    assert_eq!(plugin.version, "1.0.0");
    assert!(plugin.enabled);
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

    let plugin = PluginStore::new(db)
        .get_by_name("rich-plugin")
        .unwrap()
        .unwrap();
    assert_eq!(plugin.author.as_deref(), Some("Bob"));
    assert_eq!(plugin.description.as_deref(), Some("Does rich things"));
    assert!(!plugin.capabilities.is_empty());
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

    run(
        PluginAction::Install {
            path: plugin_path.clone(),
        },
        db.clone(),
    )
    .unwrap();

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

    let plugin = PluginStore::new(db)
        .get_by_name("enableable")
        .unwrap()
        .unwrap();
    assert!(plugin.enabled);
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

    let plugin = PluginStore::new(db)
        .get_by_name("disableable")
        .unwrap()
        .unwrap();
    assert!(!plugin.enabled);
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

    let plugin = PluginStore::new(db.clone())
        .get_by_name("toggle-dispatch")
        .unwrap()
        .unwrap();
    assert!(!plugin.enabled);

    run(
        PluginAction::Enable {
            name: "toggle-dispatch".to_string(),
        },
        db.clone(),
    )
    .unwrap();

    let plugin = PluginStore::new(db)
        .get_by_name("toggle-dispatch")
        .unwrap()
        .unwrap();
    assert!(plugin.enabled);
}
