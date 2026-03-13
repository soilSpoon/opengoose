use crate::plugin::manifest::validate_manifest;

use super::*;

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
