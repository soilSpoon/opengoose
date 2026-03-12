use std::sync::Arc;

use opengoose_persistence::{Database, PluginStore};
use opengoose_profiles::SkillStore;

use super::catalog::build_page_with_skill_store;
use super::*;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

#[test]
fn load_plugins_page_empty_returns_placeholder() {
    let page = load_plugins_page(test_db(), None).unwrap();
    assert!(page.plugins.is_empty());
    assert!(page.selected.is_placeholder);
    assert_eq!(page.mode_tone, "neutral");
    assert_eq!(page.filters.len(), 4);
}

#[test]
fn load_plugins_page_selects_first_plugin_by_default() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();
    store
        .install("beta", "2.0.0", "/tmp/b", Some("Dev"), None, "")
        .unwrap();

    let page = load_plugins_page(db, None).unwrap();
    assert_eq!(page.plugins.len(), 2);
    assert_eq!(page.selected.title, "alpha");
    assert!(page.plugins[0].active);
}

#[test]
fn load_plugins_page_named_selection_marks_active_item() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();
    store
        .install(
            "beta",
            "2.0.0",
            "/tmp/b",
            Some("Dev"),
            None,
            "skill,channel_adapter",
        )
        .unwrap();

    let page = load_plugins_page(db, Some("beta".into())).unwrap();
    assert_eq!(page.selected.title, "beta");
    assert!(
        page.plugins
            .iter()
            .find(|plugin| plugin.title == "beta")
            .unwrap()
            .active
    );
}

#[test]
fn install_plugin_from_path_requires_path() {
    let page = install_plugin_from_path(
        test_db(),
        PluginInstallInput {
            source_path: "  ".into(),
        },
    )
    .unwrap();

    assert!(page.selected.is_placeholder);
    assert_eq!(
        page.selected.notice.unwrap().text,
        "Plugin path is required."
    );
}

#[test]
fn toggle_plugin_state_updates_notice_and_status() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();

    let page = toggle_plugin_state(db.clone(), "alpha".into()).unwrap();
    assert_eq!(page.selected.lifecycle_label, "Disabled");

    let toggled = PluginStore::new(db).get_by_name("alpha").unwrap().unwrap();
    assert!(!toggled.enabled);
}

#[test]
fn delete_plugin_requires_confirmation() {
    let db = test_db();
    PluginStore::new(db.clone())
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();

    let page = delete_plugin(db, "alpha".into(), false).unwrap();
    assert_eq!(page.selected.title, "alpha");
    assert_eq!(page.selected.notice.unwrap().tone, "danger");
}

#[test]
fn filtered_page_uses_runtime_snapshot_state() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = SkillStore::with_dir(temp.path().join("skills"));
    let ready_dir = temp.path().join("ready-tools");
    let missing_dir = temp.path().join("missing-tools");

    std::fs::create_dir_all(&ready_dir).expect("ready plugin dir");
    std::fs::write(
        ready_dir.join("plugin.toml"),
        r#"
name = "ready-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "ls"
cmd = "ls"
"#,
    )
    .expect("ready manifest");
    std::fs::create_dir_all(&missing_dir).expect("missing plugin dir");
    std::fs::write(
        missing_dir.join("plugin.toml"),
        r#"
name = "missing-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "grep"
cmd = "grep"
"#,
    )
    .expect("missing manifest");

    let ready_manifest =
        opengoose_teams::plugin::load_manifest(&ready_dir.join("plugin.toml")).unwrap();
    let ready_loaded =
        opengoose_teams::plugin::LoadedPlugin::from_manifest(ready_manifest, ready_dir.clone());
    opengoose_teams::plugin::PluginRuntime::init_plugin(&ready_loaded, &skill_store).unwrap();

    store
        .install(
            "ready-tools",
            "1.0.0",
            &ready_dir.to_string_lossy(),
            None,
            Some("Ready plugin"),
            "skill",
        )
        .unwrap();
    store
        .install(
            "missing-tools",
            "1.0.0",
            &missing_dir.to_string_lossy(),
            None,
            Some("Needs registration"),
            "skill",
        )
        .unwrap();
    store
        .install("disabled-tools", "1.0.0", "/tmp/disabled", None, None, "")
        .unwrap();
    store.set_enabled("disabled-tools", false).unwrap();

    let page = build_page_with_skill_store(
        db,
        None,
        PluginStatusFilter::Attention,
        None,
        String::new(),
        Some(&skill_store),
    )
    .unwrap();

    assert_eq!(page.plugins.len(), 1);
    assert_eq!(page.plugins[0].title, "missing-tools");
    assert_eq!(page.plugins[0].status_label, "Missing skills");
    assert!(page.plugins[0].page_url.contains("status=attention"));
    assert_eq!(page.selected.runtime_label, "1 skill(s) missing");
    assert_eq!(page.mode_label, "1 operational · 1 attention · 1 disabled");
}

#[test]
fn filtered_page_shows_placeholder_when_nothing_matches() {
    let db = test_db();
    PluginStore::new(db.clone())
        .install("alpha", "1.0.0", "/tmp/a", None, None, "")
        .unwrap();

    let page = build_page_with_skill_store(
        db,
        None,
        PluginStatusFilter::Disabled,
        None,
        String::new(),
        None,
    )
    .unwrap();

    assert!(page.plugins.is_empty());
    assert!(page.selected.is_placeholder);
    assert_eq!(page.selected.title, "No disabled plugins");
}
