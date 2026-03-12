use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::render::preview_text;
use super::*;
use opengoose_teams::{OrchestrationPattern, RouterStrategy, TeamStore};

async fn test_execute(action: TeamAction, output: CliOutput) -> Result<()> {
    let tmp = tempfile::tempdir().unwrap();
    let store = TeamStore::with_dir(tmp.path().to_path_buf());
    execute_with_store(action, store, output).await
}

fn text_output() -> CliOutput {
    CliOutput::new(crate::cmd::output::OutputMode::Text)
}

fn json_output() -> CliOutput {
    CliOutput::new(crate::cmd::output::OutputMode::Json)
}

async fn execute_in_store_dir(action: TeamAction, dir: &Path, output: CliOutput) -> Result<()> {
    execute_with_store(action, TeamStore::with_dir(dir.to_path_buf()), output).await
}

fn write_team_file(contents: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{contents}").unwrap();
    file
}

fn chain_team_yaml(name: &str) -> String {
    format!(
        r#"version: "1.0.0"
title: "{name}"
description: "Custom team"
workflow: chain
agents:
  - profile: developer
    role: "Implement the change"
"#
    )
}

fn router_team_yaml(name: &str) -> String {
    format!(
        r#"version: "1.0.0"
title: "{name}"
description: "Router team"
workflow: router
agents:
  - profile: triager
    role: "Route work to the right specialist"
router:
  strategy: content-based
"#
    )
}

#[tokio::test]
async fn add_reports_file_not_found() {
    let err = test_execute(
        TeamAction::Add {
            path: PathBuf::from("/nonexistent/path/team.yaml"),
            force: false,
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("file not found") || msg.contains("not found"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn show_reports_unknown_team() {
    let err = test_execute(
        TeamAction::Show {
            name: "definitely-nonexistent-team-xyz".into(),
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn remove_reports_unknown_team() {
    let err = test_execute(
        TeamAction::Remove {
            name: "definitely-nonexistent-team-xyz".into(),
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn list_succeeds() {
    test_execute(TeamAction::List, text_output()).await.unwrap();
}

#[tokio::test]
async fn list_json_mode_succeeds() {
    test_execute(TeamAction::List, json_output()).await.unwrap();
}

#[tokio::test]
async fn init_succeeds() {
    test_execute(TeamAction::Init { force: false }, text_output())
        .await
        .unwrap();
}

#[tokio::test]
async fn init_json_mode_succeeds() {
    test_execute(TeamAction::Init { force: false }, json_output())
        .await
        .unwrap();
}

#[tokio::test]
async fn show_json_mode_reports_unknown_team() {
    let err = test_execute(
        TeamAction::Show {
            name: "definitely-nonexistent-team-xyz".into(),
        },
        json_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_with_invalid_yaml_content_fails() {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(tmp, "this is: not: valid: yaml: {{{{").unwrap();
    let path = tmp.path().to_path_buf();

    let err = test_execute(TeamAction::Add { path, force: false }, text_output())
        .await
        .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("yaml") || msg.contains("parse") || msg.contains("invalid"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_persists_valid_team_definition() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(&chain_team_yaml("custom-team"));

    execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let team = TeamStore::with_dir(store_dir.path().to_path_buf())
        .get("custom-team")
        .unwrap();
    assert_eq!(team.title, "custom-team");
    assert_eq!(team.description.as_deref(), Some("Custom team"));
    assert_eq!(team.agents.len(), 1);
}

#[tokio::test]
async fn add_rejects_duplicate_team_without_force() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(&chain_team_yaml("duplicate-team"));
    let action = TeamAction::Add {
        path: team_file.path().to_path_buf(),
        force: false,
    };

    execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let err = execute_in_store_dir(action, store_dir.path(), text_output())
        .await
        .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("already exists"),
        "unexpected duplicate-team error: {msg}"
    );
}

#[tokio::test]
async fn add_force_overwrites_existing_team_definition() {
    let store_dir = tempfile::tempdir().unwrap();
    let initial = write_team_file(&chain_team_yaml("overwrite-team"));
    let updated = write_team_file(
        r#"version: "1.0.0"
title: "overwrite-team"
description: "Updated team"
workflow: router
agents:
  - profile: triager
    role: "Pick the right teammate"
router:
  strategy: content-based
"#,
    );

    execute_in_store_dir(
        TeamAction::Add {
            path: initial.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    execute_in_store_dir(
        TeamAction::Add {
            path: updated.path().to_path_buf(),
            force: true,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let team = TeamStore::with_dir(store_dir.path().to_path_buf())
        .get("overwrite-team")
        .unwrap();
    assert_eq!(team.description.as_deref(), Some("Updated team"));
    assert_eq!(team.workflow, OrchestrationPattern::Router);
    assert_eq!(team.router.unwrap().strategy, RouterStrategy::ContentBased);
}

#[tokio::test]
async fn add_rejects_empty_title_validation_error() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(
        r#"version: "1.0.0"
title: "   "
workflow: chain
agents:
  - profile: developer
"#,
    );

    let err = execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(msg.contains("title is required"), "unexpected error: {msg}");
}

#[tokio::test]
async fn add_rejects_empty_agents_validation_error() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(
        r#"version: "1.0.0"
title: "empty-agents"
workflow: chain
agents: []
"#,
    );

    let err = execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("at least one agent is required"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_rejects_empty_agent_profile_validation_error() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(
        r#"version: "1.0.0"
title: "bad-profile"
workflow: chain
agents:
  - profile: "   "
"#,
    );

    let err = execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("agent profile name cannot be empty"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_router_team_requires_router_config() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(
        r#"version: "1.0.0"
title: "router-without-config"
workflow: router
agents:
  - profile: triager
    role: "Pick the right teammate"
"#,
    );

    let err = execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("router workflow requires"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_router_team_with_config_succeeds() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(&router_team_yaml("router-team"));

    execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let team = TeamStore::with_dir(store_dir.path().to_path_buf())
        .get("router-team")
        .unwrap();
    assert_eq!(team.workflow, OrchestrationPattern::Router);
    assert_eq!(team.router.unwrap().strategy, RouterStrategy::ContentBased);
}

#[tokio::test]
async fn remove_existing_team_deletes_definition() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_file = write_team_file(&chain_team_yaml("remove-me"));

    execute_in_store_dir(
        TeamAction::Add {
            path: team_file.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    execute_in_store_dir(
        TeamAction::Remove {
            name: "remove-me".into(),
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let err = TeamStore::with_dir(store_dir.path().to_path_buf())
        .get("remove-me")
        .unwrap_err();
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected post-remove error: {msg}"
    );
}

#[tokio::test]
async fn add_reports_file_not_found_in_json_mode() {
    let err = test_execute(
        TeamAction::Add {
            path: PathBuf::from("/nonexistent/path/team.yaml"),
            force: false,
        },
        json_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("file not found") || msg.contains("not found"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn remove_json_mode_reports_unknown_team() {
    let err = test_execute(
        TeamAction::Remove {
            name: "definitely-nonexistent-team-xyz".into(),
        },
        json_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("does not exist"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_empty_file_fails() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let err = test_execute(TeamAction::Add { path, force: false }, text_output())
        .await
        .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("yaml")
            || msg.contains("parse")
            || msg.contains("invalid")
            || msg.contains("missing"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_with_force_flag_file_not_found() {
    let err = test_execute(
        TeamAction::Add {
            path: PathBuf::from("/nonexistent/path/team.yaml"),
            force: true,
        },
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("file not found") || msg.contains("not found"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn add_rejects_directory_path() {
    let store_dir = tempfile::tempdir().unwrap();

    let err = execute_in_store_dir(
        TeamAction::Add {
            path: store_dir.path().to_path_buf(),
            force: false,
        },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap_err();

    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("directory") || msg.contains("is a directory"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn init_force_overwrites_existing_default_team() {
    let store_dir = tempfile::tempdir().unwrap();
    let team_path = store_dir.path().join("code-review.yaml");
    std::fs::write(
        &team_path,
        r#"version: "1.0.0"
title: "code-review"
description: "Custom override"
workflow: chain
agents:
  - profile: developer
    role: "Override"
"#,
    )
    .unwrap();

    execute_in_store_dir(
        TeamAction::Init { force: true },
        store_dir.path(),
        text_output(),
    )
    .await
    .unwrap();

    let team = TeamStore::with_dir(store_dir.path().to_path_buf())
        .get("code-review")
        .unwrap();
    assert_eq!(
        team.description.as_deref(),
        Some("Developer writes code, reviewer provides feedback in a sequential chain.")
    );
}

#[test]
fn preview_text_truncates_multibyte_strings_on_char_boundary() {
    assert_eq!(preview_text("가나다", 4), "가...");
}

#[test]
fn team_store_new_succeeds() {
    let store = TeamStore::new();
    assert!(store.is_ok());
}

#[test]
fn team_store_get_nonexistent_returns_error() {
    let store = TeamStore::new().unwrap();
    let result = store.get("nonexistent-team-xyz-12345");
    assert!(result.is_err());
}

#[test]
fn team_store_list_returns_vec() {
    let store = TeamStore::new().unwrap();
    let names = store.list();
    assert!(names.is_ok());
}
