use std::sync::Arc;

use opengoose_persistence::{Database, PluginStore};

use super::*;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("in-memory db"))
}

#[test]
fn skill_plugin_reports_initialized_when_declared_skills_are_registered() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = opengoose_profiles::SkillStore::with_dir(temp.path().join("skills"));
    let plugin_dir = temp.path().join("file-tools");
    write_manifest(
        &plugin_dir,
        r#"
name = "file-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "ls"
cmd = "ls"
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).expect("manifest");
    let loaded = LoadedPlugin::from_manifest(manifest, plugin_dir.clone());
    PluginRuntime::init_plugin(&loaded, &skill_store).expect("runtime init should succeed");
    let plugin = store
        .install(
            "file-tools",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "skill",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, Some(&skill_store));
    assert!(snapshot.runtime_initialized);
    assert_eq!(snapshot.registered_skills, vec!["file-tools/ls"]);
    assert!(snapshot.missing_skills.is_empty());
    assert_eq!(
        snapshot.runtime_note.as_deref(),
        Some("registered 1 declared skill(s)")
    );
}

#[test]
fn skill_plugin_reports_missing_runtime_registration() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = opengoose_profiles::SkillStore::with_dir(temp.path().join("skills"));
    let plugin_dir = temp.path().join("missing-tools");
    write_manifest(
        &plugin_dir,
        r#"
name = "missing-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "grep"
cmd = "grep"
"#,
    );

    let plugin = store
        .install(
            "missing-tools",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "skill",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, Some(&skill_store));
    assert!(!snapshot.runtime_initialized);
    assert!(snapshot.registered_skills.is_empty());
    assert_eq!(snapshot.missing_skills, vec!["missing-tools/grep"]);
    assert_eq!(
        snapshot.runtime_note.as_deref(),
        Some("missing 1 of 1 declared skill(s)")
    );
}

#[test]
fn skill_plugin_reports_unavailable_skill_store() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let plugin_dir = temp.path().join("offline-tools");
    write_manifest(
        &plugin_dir,
        r#"
name = "offline-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "find"
cmd = "find"
"#,
    );

    let plugin = store
        .install(
            "offline-tools",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "skill",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, None);
    assert!(!snapshot.runtime_initialized);
    assert!(snapshot.registered_skills.is_empty());
    assert_eq!(snapshot.missing_skills, vec!["offline-tools/find"]);
    assert_eq!(
        snapshot.runtime_note.as_deref(),
        Some("skill store unavailable; runtime registration could not be verified")
    );
}

#[test]
fn channel_adapter_plugin_reports_unsupported_runtime_loading() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let plugin_dir = temp.path().join("matrix-adapter");
    write_manifest(
        &plugin_dir,
        r#"
name = "matrix-adapter"
version = "1.0.0"
capabilities = ["channel_adapter"]
"#,
    );

    let plugin = store
        .install(
            "matrix-adapter",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "channel_adapter",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, None);
    assert!(!snapshot.runtime_initialized);
    assert_eq!(snapshot.capabilities, vec!["channel_adapter"]);
    assert_eq!(
        snapshot.runtime_note.as_deref(),
        Some("channel adapter runtime loading is not implemented yet")
    );
}

#[test]
fn plugin_without_runtime_capability_reports_explicit_note() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = opengoose_profiles::SkillStore::with_dir(temp.path().join("skills"));
    let plugin_dir = temp.path().join("docs-plugin");
    write_manifest(
        &plugin_dir,
        r#"
name = "docs-plugin"
version = "1.0.0"
"#,
    );

    let plugin = store
        .install(
            "docs-plugin",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, Some(&skill_store));
    assert!(!snapshot.runtime_initialized);
    assert!(snapshot.registered_skills.is_empty());
    assert!(snapshot.missing_skills.is_empty());
    assert_eq!(
        snapshot.runtime_note.as_deref(),
        Some("plugin does not declare a runtime capability")
    );
}

#[test]
fn manifest_capabilities_override_persisted_capabilities_in_snapshot() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = opengoose_profiles::SkillStore::with_dir(temp.path().join("skills"));
    let plugin_dir = temp.path().join("dual-runtime");
    write_manifest(
        &plugin_dir,
        r#"
name = "dual-runtime"
version = "1.0.0"
capabilities = ["skill", "channel_adapter"]

[[skills]]
name = "echo"
cmd = "echo"
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).expect("manifest");
    let loaded = LoadedPlugin::from_manifest(manifest, plugin_dir.clone());
    PluginRuntime::init_plugin(&loaded, &skill_store).expect("runtime init should succeed");
    let plugin = store
        .install(
            "dual-runtime",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "skill",
        )
        .expect("plugin should install");

    let snapshot = plugin_status_snapshot(&plugin, Some(&skill_store));
    assert_eq!(snapshot.capabilities, vec!["skill", "channel_adapter"]);
    assert!(snapshot.runtime_initialized);
}

#[test]
fn snapshot_listing_falls_back_when_manifest_is_missing() {
    let db = test_db();
    let store = PluginStore::new(db);
    let temp = tempfile::tempdir().expect("temp dir");
    let plugin_dir = temp.path().join("broken-plugin");
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir should exist");

    store
        .install(
            "broken-plugin",
            "1.0.0",
            &plugin_dir.to_string_lossy(),
            None,
            None,
            "skill,channel_adapter",
        )
        .expect("plugin should install");

    let snapshots = list_plugin_status_snapshots(&store, None).expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].capabilities,
        vec!["skill".to_string(), "channel_adapter".to_string()]
    );
    assert!(!snapshots[0].runtime_initialized);
    assert!(
        snapshots[0]
            .runtime_note
            .as_deref()
            .is_some_and(|note| note.contains("plugin manifest unavailable"))
    );
}
