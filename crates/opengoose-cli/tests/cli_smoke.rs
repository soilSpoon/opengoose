use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn test_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let goose_root = temp.path().join("goose");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&goose_root).unwrap();
    (temp, home, goose_root)
}

fn run_cli(home: &Path, goose_root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opengoose"))
        .args(args)
        .env("HOME", home)
        .env("GOOSE_PATH_ROOT", goose_root)
        .env("GOOSE_DISABLE_KEYRING", "1")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn profile_commands_work_end_to_end() {
    let (_temp, home, goose_root) = test_env();

    let empty_list = run_cli(&home, &goose_root, &["profile", "list"]);
    assert!(empty_list.status.success());
    assert!(stdout(&empty_list).contains("No profiles found."));

    let init = run_cli(&home, &goose_root, &["profile", "init"]);
    assert!(init.status.success());
    assert!(stdout(&init).contains("default profile(s)."));

    let second_init = run_cli(&home, &goose_root, &["profile", "init"]);
    assert!(second_init.status.success());
    assert!(stdout(&second_init).contains("All default profiles already exist."));

    let list = run_cli(&home, &goose_root, &["profile", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("Agent profiles:"));
    assert!(list_stdout.contains("developer"));
    assert!(list_stdout.contains("reviewer"));

    let show = run_cli(&home, &goose_root, &["profile", "show", "developer"]);
    assert!(show.status.success());
    let show_stdout = stdout(&show);
    assert!(show_stdout.contains("title: developer"));
    assert!(show_stdout.contains("description:"));

    let remove = run_cli(&home, &goose_root, &["profile", "remove", "developer"]);
    assert!(remove.status.success());
    assert!(stdout(&remove).contains("Removed profile `developer`."));

    let missing = run_cli(&home, &goose_root, &["profile", "show", "developer"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("profile `developer` not found"));
}

#[test]
fn profile_add_loads_custom_yaml_file() {
    let (_temp, home, goose_root) = test_env();
    let profile_path = home.join("custom-profile.yaml");
    fs::write(
        &profile_path,
        r#"version: "1.0.0"
title: "custom-profile"
description: "Custom profile"
prompt: "Be useful"
"#,
    )
    .unwrap();

    let add = run_cli(
        &home,
        &goose_root,
        &["profile", "add", profile_path.to_str().unwrap()],
    );
    assert!(add.status.success());
    assert!(stdout(&add).contains("Added profile `custom-profile`."));

    let show = run_cli(&home, &goose_root, &["profile", "show", "custom-profile"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("title: custom-profile"));
}

#[test]
fn team_commands_work_end_to_end() {
    let (_temp, home, goose_root) = test_env();

    let empty_list = run_cli(&home, &goose_root, &["team", "list"]);
    assert!(empty_list.status.success());
    assert!(stdout(&empty_list).contains("No teams found."));

    let init = run_cli(&home, &goose_root, &["team", "init"]);
    assert!(init.status.success());
    assert!(stdout(&init).contains("default team(s)."));

    let second_init = run_cli(&home, &goose_root, &["team", "init"]);
    assert!(second_init.status.success());
    assert!(stdout(&second_init).contains("All default teams already exist."));

    let list = run_cli(&home, &goose_root, &["team", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("Teams:"));
    assert!(list_stdout.contains("code-review"));
    assert!(list_stdout.contains("smart-router"));

    let show = run_cli(&home, &goose_root, &["team", "show", "code-review"]);
    assert!(show.status.success());
    let show_stdout = stdout(&show);
    assert!(show_stdout.contains("title: code-review"));
    assert!(show_stdout.contains("workflow: chain"));

    let remove = run_cli(&home, &goose_root, &["team", "remove", "code-review"]);
    assert!(remove.status.success());
    assert!(stdout(&remove).contains("Removed team `code-review`."));

    let missing = run_cli(&home, &goose_root, &["team", "show", "code-review"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("team `code-review` not found"));
}

#[test]
fn team_add_loads_custom_yaml_file() {
    let (_temp, home, goose_root) = test_env();
    let team_path = home.join("custom-team.yaml");
    fs::write(
        &team_path,
        r#"version: "1.0.0"
title: "custom-team"
description: "Custom team"
workflow: chain
agents:
  - profile: developer
    role: "Implement the change"
"#,
    )
    .unwrap();

    let add = run_cli(
        &home,
        &goose_root,
        &["team", "add", team_path.to_str().unwrap()],
    );
    assert!(add.status.success());
    assert!(stdout(&add).contains("Added team `custom-team`."));

    let show = run_cli(&home, &goose_root, &["team", "show", "custom-team"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("title: custom-team"));
}

#[test]
fn auth_list_and_models_error_paths_work() {
    let (_temp, home, goose_root) = test_env();

    let list = run_cli(&home, &goose_root, &["auth", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("PROVIDER"));
    assert!(list_stdout.contains("STATUS"));

    let models = run_cli(
        &home,
        &goose_root,
        &["auth", "models", "definitely-unknown-provider"],
    );
    assert!(!models.status.success());
    let models_stderr = stderr(&models);
    assert!(models_stderr.contains("Fetching models for definitely-unknown-provider"));
    assert!(models_stderr.contains("Unknown provider: definitely-unknown-provider"));
}
