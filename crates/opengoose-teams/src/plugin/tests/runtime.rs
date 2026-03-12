use super::*;

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
