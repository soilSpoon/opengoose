use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
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

fn stdout_json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap()
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_str(&stderr(output)).unwrap()
}

fn assert_runtime_error_message(output: &Output, kind: &str, expected_message: &str) {
    assert!(!output.status.success());
    let error = &stderr_json(output)["error"];
    assert_eq!(error["kind"], Value::from(kind));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains(expected_message)),
        "unexpected error payload: {error}"
    );
}

#[test]
fn auth_list_text_includes_ready_local_provider() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["auth", "list"]);
    assert!(output.status.success());

    let text = stdout(&output);
    assert!(text.contains("Providers"));
    assert!(text.contains("Local Inference"));
    assert!(text.contains("ready"));
}

#[test]
fn auth_list_json_reports_local_provider_ready() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["--json", "auth", "list"]);
    assert!(output.status.success());

    let body = stdout_json(&output);
    assert_eq!(body["ok"], Value::from(true));
    assert_eq!(body["command"], Value::from("auth.list"));

    let providers = body["providers"].as_array().unwrap();
    let local = providers
        .iter()
        .find(|provider| provider["name"] == "local")
        .expect("local provider should be listed");
    assert_eq!(local["display_name"], Value::from("Local Inference"));
    assert_eq!(local["auth"], Value::from("none"));
    assert_eq!(local["status"], Value::from("ready"));
}

#[test]
fn auth_list_json_reports_clean_home_without_custom_secrets() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["--json", "auth", "list"]);
    assert!(output.status.success());

    let body = stdout_json(&output);
    assert_eq!(body["custom_secrets_configured"], Value::from(false));
}

#[test]
fn auth_login_local_json_reports_ready_without_credentials() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["--json", "auth", "login", "local"]);
    assert!(output.status.success());

    let body = stdout_json(&output);
    assert_eq!(body["ok"], Value::from(true));
    assert_eq!(body["command"], Value::from("auth.login"));
    assert_eq!(body["provider"], Value::from("local"));
    assert_eq!(body["display_name"], Value::from("Local Inference"));
    assert_eq!(body["status"], Value::from("ready"));
}

#[test]
fn auth_login_local_text_reports_no_auth_requirement() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["auth", "login", "local"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains(
        "Local Inference does not require authentication. Just set it as your provider."
    ));
}

#[test]
fn auth_login_unknown_provider_json_emits_structured_error() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(
        &home,
        &goose_root,
        &["--json", "auth", "login", "definitely-unknown-provider"],
    );
    assert_runtime_error_message(&output, "invalid_input", "unknown provider");
}

#[test]
fn auth_logout_unknown_provider_json_emits_structured_error() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(
        &home,
        &goose_root,
        &["--json", "auth", "logout", "definitely-unknown-provider"],
    );
    assert_runtime_error_message(&output, "invalid_input", "no stored credentials found");
}

#[test]
fn auth_models_local_json_returns_known_models() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["--json", "auth", "models", "local"]);
    assert!(output.status.success());

    let body = stdout_json(&output);
    assert_eq!(body["ok"], Value::from(true));
    assert_eq!(body["command"], Value::from("auth.models"));
    assert_eq!(body["provider"], Value::from("local"));

    let models = body["models"].as_array().unwrap();
    assert!(models.len() >= 4);
    assert!(models.iter().any(|model| {
        model
            .as_str()
            .is_some_and(|name| name.contains("bartowski/"))
    }));
}

#[test]
fn auth_models_local_text_renders_model_table() {
    let (_temp, home, goose_root) = test_env();

    let output = run_cli(&home, &goose_root, &["auth", "models", "local"]);
    assert!(output.status.success());

    let text = stdout(&output);
    assert!(text.contains("Models for local"));
    assert!(text.contains("MODEL"));
    assert!(text.contains("bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M"));
}
