use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::prelude::CommandCargoExt;
use predicates::str::contains;
use tempfile::TempDir;

fn build_cmd(home: &TempDir, args: &[&str]) -> Command {
    let goose_root = home.path().join("goose");
    let mut cmd = Command::cargo_bin("opengoose").unwrap();
    cmd.args(args);
    cmd.env("HOME", home.path())
        .env("GOOSE_PATH_ROOT", &goose_root)
        .env("GOOSE_DISABLE_KEYRING", "1");
    cmd
}

#[test]
fn help_includes_web_subcommand() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("goose")).unwrap();

    build_cmd(&home, &["--help"])
        .assert()
        .success()
        .stdout(contains("Start the web dashboard server"));
}

#[test]
fn web_help_renders_usage() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("goose")).unwrap();

    build_cmd(&home, &["web", "--help"])
        .assert()
        .success()
        .stdout(contains("Start the web dashboard server"));
}

#[test]
fn profile_list_command_works_without_defaults() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("goose")).unwrap();

    build_cmd(&home, &["profile", "list"]).assert().success().stdout(
        contains("No profiles found."),
    );
}

#[test]
fn invalid_auth_models_json_emits_structured_error() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("goose")).unwrap();

    build_cmd(
        &home,
        &["--json", "auth", "models", "definitely-unknown-provider"],
    )
    .assert()
    .failure()
    .stderr(contains("\"kind\": \"invalid_input\""));
}
