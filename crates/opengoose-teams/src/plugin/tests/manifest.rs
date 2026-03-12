use super::*;

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
