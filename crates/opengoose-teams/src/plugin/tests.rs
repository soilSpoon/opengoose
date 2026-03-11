use std::path::Path;

use super::manifest::validate_manifest;
use super::*;

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
