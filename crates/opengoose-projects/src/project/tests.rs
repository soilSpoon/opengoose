use std::path::PathBuf;

use super::{ProjectContext, ProjectDefinition, ProjectSettings};

fn minimal_yaml() -> &'static str {
    r#"
version: "1.0.0"
title: "my-project"
"#
}

#[test]
fn round_trip_minimal() {
    let project = ProjectDefinition::from_yaml(minimal_yaml()).unwrap();
    assert_eq!(project.name(), "my-project");
    assert!(project.goal.is_none());
    assert!(project.cwd.is_none());
    assert!(project.context_files.is_empty());
    assert!(project.default_team.is_none());

    let serialized = project.to_yaml().unwrap();
    let reparsed = ProjectDefinition::from_yaml(&serialized).unwrap();
    assert_eq!(reparsed.title, project.title);
}

#[test]
fn round_trip_full() {
    let yaml = r#"
version: "1.0.0"
title: "opengoose"
goal: "Build the best multi-agent orchestrator"
cwd: "/workspace/opengoose"
context_files:
  - README.md
  - docs/architecture.md
default_team: code-review
description: "Main OpenGoose development project"
settings:
  max_turns: 20
  message_retention_days: 30
"#;
    let project = ProjectDefinition::from_yaml(yaml).unwrap();
    assert_eq!(
        project.goal.as_deref(),
        Some("Build the best multi-agent orchestrator")
    );
    assert_eq!(project.cwd.as_deref(), Some("/workspace/opengoose"));
    assert_eq!(
        project.context_files,
        vec!["README.md", "docs/architecture.md"]
    );
    assert_eq!(project.default_team.as_deref(), Some("code-review"));
    let settings = project.settings.as_ref().unwrap();
    assert_eq!(settings.max_turns, Some(20));
    assert_eq!(settings.message_retention_days, Some(30));

    let serialized = project.to_yaml().unwrap();
    let reparsed = ProjectDefinition::from_yaml(&serialized).unwrap();
    assert_eq!(reparsed.title, project.title);
    assert_eq!(reparsed.goal, project.goal);
    assert_eq!(reparsed.context_files, project.context_files);
}

#[test]
fn validation_rejects_empty_title() {
    let yaml = r#"
version: "1.0.0"
title: "   "
"#;
    let err = ProjectDefinition::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("title is required"));
}

#[test]
fn resolve_cwd_explicit() {
    let yaml = r#"
version: "1.0.0"
title: "p"
cwd: "/tmp/myproject"
"#;
    let project = ProjectDefinition::from_yaml(yaml).unwrap();
    assert_eq!(project.resolve_cwd(None), PathBuf::from("/tmp/myproject"));
}

#[test]
fn resolve_cwd_from_store_dir() {
    let yaml = r#"
version: "1.0.0"
title: "p"
"#;
    let project = ProjectDefinition::from_yaml(yaml).unwrap();
    let store_dir = PathBuf::from("/store/projects");
    assert_eq!(project.resolve_cwd(Some(&store_dir)), store_dir);
}

#[test]
fn resolve_cwd_fallback_to_process_cwd() {
    let yaml = r#"
version: "1.0.0"
title: "p"
"#;
    let project = ProjectDefinition::from_yaml(yaml).unwrap();
    let resolved = project.resolve_cwd(None);
    assert!(resolved.is_absolute() || !resolved.as_os_str().is_empty());
}

#[test]
fn project_context_from_definition_no_files() {
    let yaml = r#"
version: "1.0.0"
title: "test"
goal: "Test goal"
cwd: "/tmp"
"#;
    let def = ProjectDefinition::from_yaml(yaml).unwrap();
    let ctx = ProjectContext::from_definition(&def, None);
    assert_eq!(ctx.title, "test");
    assert_eq!(ctx.goal, "Test goal");
    assert_eq!(ctx.cwd, PathBuf::from("/tmp"));
    assert!(ctx.context_entries.is_empty());
}

#[test]
fn system_prompt_extension_includes_goal_and_cwd() {
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "demo".into(),
        goal: Some("Ship v2".into()),
        cwd: Some("/workspace".into()),
        context_files: vec![],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    let ext = ctx.system_prompt_extension();
    assert!(ext.contains("demo"));
    assert!(ext.contains("Ship v2"));
    assert!(ext.contains("/workspace"));
}

#[test]
fn system_prompt_extension_no_goal() {
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "minimal".into(),
        goal: None,
        cwd: Some("/tmp".into()),
        context_files: vec![],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    let ext = ctx.system_prompt_extension();
    assert!(ext.contains("minimal"));
    assert!(!ext.contains("Goal:"));
}

#[test]
fn project_context_skips_missing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "proj".into(),
        goal: None,
        cwd: Some(tmp.path().to_string_lossy().into()),
        context_files: vec!["nonexistent.md".into()],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    assert!(ctx.context_entries.is_empty());
}

#[test]
fn project_context_loads_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx_file = tmp.path().join("notes.md");
    std::fs::write(&ctx_file, "# Notes\nImportant context here.").unwrap();

    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "proj".into(),
        goal: Some("Build it".into()),
        cwd: Some(tmp.path().to_string_lossy().into()),
        context_files: vec!["notes.md".into()],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    assert_eq!(ctx.context_entries.len(), 1);
    assert_eq!(ctx.context_entries[0].0, "notes");
    assert!(ctx.context_entries[0].1.contains("Important context"));

    let ext = ctx.system_prompt_extension();
    assert!(ext.contains("notes"));
    assert!(ext.contains("Important context"));
}

#[test]
fn file_name_converts_title_to_lowercase_hyphenated_yaml() {
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "My Cool Project".into(),
        goal: None,
        cwd: None,
        context_files: vec![],
        default_team: None,
        description: None,
        settings: None,
    };
    assert_eq!(def.file_name(), "my-cool-project.yaml");
}

#[test]
fn system_prompt_extension_truncates_long_context_file() {
    let tmp = tempfile::tempdir().unwrap();
    let big_file = tmp.path().join("big.md");
    let content = "x".repeat(5000);
    std::fs::write(&big_file, &content).unwrap();

    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "proj".into(),
        goal: None,
        cwd: Some(tmp.path().to_string_lossy().into()),
        context_files: vec!["big.md".into()],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    let ext = ctx.system_prompt_extension();
    assert!(ext.contains("[truncated]"), "expected truncation marker");
    assert!(!ext.contains(&"x".repeat(5000)));
}

#[test]
fn project_settings_is_empty_returns_true_when_no_fields_set() {
    let settings = ProjectSettings::default();
    assert!(settings.is_empty());
}

#[test]
fn project_settings_is_empty_returns_false_when_any_field_set() {
    let s1 = ProjectSettings {
        max_turns: Some(5),
        message_retention_days: None,
    };
    assert!(!s1.is_empty());

    let s2 = ProjectSettings {
        max_turns: None,
        message_retention_days: Some(30),
    };
    assert!(!s2.is_empty());
}

#[test]
fn project_context_loads_file_by_absolute_path() {
    // Verify the absolute-path branch in from_definition: an absolute context_files
    // entry should be read directly (not joined with cwd).
    let tmp = tempfile::tempdir().unwrap();
    let abs_file = tmp.path().join("notes.md");
    std::fs::write(&abs_file, "absolute content").unwrap();

    // Use a *different* cwd so a relative path would resolve to the wrong place.
    let other_tmp = tempfile::tempdir().unwrap();
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "proj".into(),
        goal: None,
        cwd: Some(other_tmp.path().to_string_lossy().into()),
        context_files: vec![abs_file.to_string_lossy().into()],
        default_team: None,
        description: None,
        settings: None,
    };
    let ctx = ProjectContext::from_definition(&def, None);
    assert_eq!(ctx.context_entries.len(), 1);
    assert_eq!(ctx.context_entries[0].0, "notes");
    assert_eq!(ctx.context_entries[0].1, "absolute content");
}

#[test]
fn resolve_cwd_with_tilde_prefix() {
    // Verify that a cwd starting with "~/" is expanded to the home directory.
    let home = dirs::home_dir().expect("HOME must be set for this test");
    let def = ProjectDefinition {
        version: "1.0.0".into(),
        title: "p".into(),
        goal: None,
        cwd: Some("~/projects/myapp".into()),
        context_files: vec![],
        default_team: None,
        description: None,
        settings: None,
    };
    let resolved = def.resolve_cwd(None);
    assert_eq!(resolved, home.join("projects/myapp"));
}
