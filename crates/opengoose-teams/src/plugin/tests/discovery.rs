use super::*;

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
