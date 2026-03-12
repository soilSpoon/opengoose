use std::path::Path;
use std::sync::Arc;

use super::manifest::validate_manifest;
use super::*;
use opengoose_persistence::{Database, PluginStore};

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
    let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
    assert_eq!(manifest.name, "git-skill");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(manifest.author.as_deref(), Some("Bob"));
    assert_eq!(manifest.capabilities, vec!["skill"]);
    assert_eq!(manifest.capabilities_str(), "skill");
}

#[test]
fn test_manifest_minimal() {
    let toml_str = r#"
name = "minimal"
version = "0.1.0"
"#;
    let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
    assert!(manifest.author.is_none());
    assert!(manifest.capabilities.is_empty());
    assert_eq!(manifest.capabilities_str(), "");
    assert!(manifest.skills.is_empty());
}

#[test]
fn test_manifest_with_skills() {
    let toml_str = r#"
name = "git-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "git-log"
cmd = "git"
args = ["log", "--oneline", "-20"]
description = "Show recent commits"
timeout = 30

[[skills]]
name = "git-status"
cmd = "git"
args = ["status"]
"#;
    let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
    assert_eq!(manifest.skills.len(), 2);
    assert_eq!(manifest.skills[0].name, "git-log");
    assert_eq!(manifest.skills[0].cmd, "git");
    assert_eq!(manifest.skills[0].args, vec!["log", "--oneline", "-20"]);
    assert_eq!(
        manifest.skills[0].description.as_deref(),
        Some("Show recent commits")
    );
    assert_eq!(manifest.skills[0].timeout, Some(30));
    assert_eq!(manifest.skills[1].name, "git-status");
    assert!(manifest.skills[1].description.is_none());
    assert_eq!(manifest.skills[1].timeout, None);
}

#[test]
fn test_manifest_skill_with_envs() {
    let toml_str = r#"
name = "env-plugin"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "custom-tool"
cmd = "my-tool"
envs = { API_KEY = "test", MODE = "production" }
"#;
    let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
    assert_eq!(manifest.skills[0].envs.len(), 2);
    assert_eq!(manifest.skills[0].envs.get("API_KEY").unwrap(), "test");
    assert_eq!(manifest.skills[0].envs.get("MODE").unwrap(), "production");
}

#[test]
fn test_validate_rejects_empty_name() {
    let manifest = PluginManifest {
        name: "  ".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        capabilities: vec![],
        skills: vec![],
    };
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn test_validate_rejects_empty_version() {
    let manifest = PluginManifest {
        name: "ok".into(),
        version: "".into(),
        author: None,
        description: None,
        capabilities: vec![],
        skills: vec![],
    };
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn test_validate_rejects_empty_skill_name() {
    let manifest = PluginManifest {
        name: "test".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        capabilities: vec!["skill".into()],
        skills: vec![PluginSkillDef {
            name: "  ".into(),
            cmd: "echo".into(),
            args: vec![],
            description: None,
            timeout: None,
            envs: Default::default(),
        }],
    };
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn test_validate_rejects_empty_skill_cmd() {
    let manifest = PluginManifest {
        name: "test".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        capabilities: vec!["skill".into()],
        skills: vec![PluginSkillDef {
            name: "my-skill".into(),
            cmd: "".into(),
            args: vec![],
            description: None,
            timeout: None,
            envs: Default::default(),
        }],
    };
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn test_has_skill_capability() {
    let manifest = PluginManifest {
        name: "test".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        capabilities: vec!["skill".into(), "other".into()],
        skills: vec![],
    };
    assert!(manifest.has_skill_capability());
    assert!(!manifest.has_channel_adapter_capability());
}

#[test]
fn test_has_channel_adapter_capability() {
    let manifest = PluginManifest {
        name: "test".into(),
        version: "1.0.0".into(),
        author: None,
        description: None,
        capabilities: vec!["channel_adapter".into()],
        skills: vec![],
    };
    assert!(!manifest.has_skill_capability());
    assert!(manifest.has_channel_adapter_capability());
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
    let plugin_dir = std::env::temp_dir().join("opengoose-loaded-plugin-trait");
    let loaded = LoadedPlugin::new(
        PluginManifest {
            name: "my-plugin".into(),
            version: "1.0.0".into(),
            author: None,
            description: None,
            capabilities: vec!["skill".into(), "channel_adapter".into()],
            skills: vec![],
        },
        plugin_dir.clone(),
    );

    assert_eq!(loaded.name(), "my-plugin");
    assert_eq!(loaded.version(), "1.0.0");
    assert_eq!(loaded.capabilities(), "skill, channel_adapter");
    assert_eq!(loaded.source_path(), plugin_dir.as_path());
    assert!(loaded.init().is_ok());
    assert!(loaded.shutdown().is_ok());
}

#[test]
fn test_plugin_runtime_init_registers_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("my-plugin");
    write_manifest(
        &plugin_dir,
        r#"
name = "git-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "git-log"
cmd = "git"
args = ["log", "--oneline"]
description = "Recent commits"
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);

    let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
    assert_eq!(result.plugin_name, "git-tools");
    assert_eq!(result.registered_skills, vec!["git-tools/git-log"]);

    let skill = skill_store.get("git-tools/git-log").unwrap();
    assert_eq!(skill.version, "1.0.0");
    assert_eq!(skill.extensions.len(), 1);
    assert_eq!(skill.extensions[0].name, "git-log");
    assert_eq!(skill.extensions[0].cmd.as_deref(), Some("git"));
    assert_eq!(skill.extensions[0].args, vec!["log", "--oneline"]);
}

#[test]
fn test_plugin_runtime_init_multiple_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("multi");
    write_manifest(
        &plugin_dir,
        r#"
name = "multi-tool"
version = "2.0.0"
capabilities = ["skill"]

[[skills]]
name = "tool-a"
cmd = "echo"
args = ["a"]

[[skills]]
name = "tool-b"
cmd = "echo"
args = ["b"]
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);

    let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
    assert_eq!(result.registered_skills.len(), 2);
    assert_eq!(result.registered_skills[0], "multi-tool/tool-a");
    assert_eq!(result.registered_skills[1], "multi-tool/tool-b");

    assert!(skill_store.get("multi-tool/tool-a").is_ok());
    assert!(skill_store.get("multi-tool/tool-b").is_ok());
}

#[test]
fn test_plugin_runtime_init_no_skills_capability() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("adapter");
    write_manifest(
        &plugin_dir,
        r#"
name = "my-adapter"
version = "1.0.0"
capabilities = ["channel_adapter"]
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);

    let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
    assert!(result.registered_skills.is_empty());
}

#[test]
fn test_plugin_runtime_shutdown_removes_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("removable");
    write_manifest(
        &plugin_dir,
        r#"
name = "removable"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "tool-x"
cmd = "echo"
args = ["x"]
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);

    PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
    assert!(skill_store.get("removable/tool-x").is_ok());

    let removed = PluginRuntime::shutdown_plugin(&loaded, &skill_store).unwrap();
    assert_eq!(removed, vec!["removable/tool-x"]);
    assert!(skill_store.get("removable/tool-x").is_err());
}

#[test]
fn test_plugin_runtime_shutdown_nonexistent_skill_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("ghost");
    write_manifest(
        &plugin_dir,
        r#"
name = "ghost-plugin"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "phantom"
cmd = "echo"
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);

    let removed = PluginRuntime::shutdown_plugin(&loaded, &skill_store).unwrap();
    assert!(removed.is_empty());
}

#[test]
fn test_plugin_runtime_init_overwrites_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills");
    let skill_store = opengoose_profiles::SkillStore::with_dir(skill_dir);

    let plugin_dir = tmp.path().join("updatable");
    write_manifest(
        &plugin_dir,
        r#"
name = "updatable"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "my-tool"
cmd = "echo"
args = ["v1"]
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir.clone());
    PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();

    write_manifest(
        &plugin_dir,
        r#"
name = "updatable"
version = "2.0.0"
capabilities = ["skill"]

[[skills]]
name = "my-tool"
cmd = "echo"
args = ["v2"]
"#,
    );

    let manifest = load_manifest(&plugin_dir.join("plugin.toml")).unwrap();
    let loaded = LoadedPlugin::new(manifest, plugin_dir);
    let result = PluginRuntime::init_plugin(&loaded, &skill_store).unwrap();
    assert_eq!(result.registered_skills, vec!["updatable/my-tool"]);

    let skill = skill_store.get("updatable/my-tool").unwrap();
    assert_eq!(skill.version, "2.0.0");
    assert_eq!(skill.extensions[0].args, vec!["v2"]);
}

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
