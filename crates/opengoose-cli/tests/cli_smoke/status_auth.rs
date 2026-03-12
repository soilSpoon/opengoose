use opengoose_persistence::EventStore;
use opengoose_types::{AppEventKind, Platform, SessionKey};
use serde_json::Value;

use crate::support::{
    CliHarness, assert_runtime_error_message, open_database, stderr, stderr_json, stdout,
    stdout_json,
};

#[test]
fn run_command_rejects_json_output() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "run"]);
    assert_runtime_error_message(&output, "unsupported_output", "does not support --json");
}

#[test]
fn team_status_without_runs_reports_empty_state() {
    let harness = CliHarness::new();

    let output = harness.run(&["team", "status"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("No team runs found."));
}

#[test]
fn team_status_missing_run_reports_not_found() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "team", "status", "missing-run"]);
    assert_runtime_error_message(&output, "runtime_error", "run 'missing-run' not found");
}

#[test]
fn team_logs_missing_run_reports_not_found() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "team", "logs", "missing-run"]);
    assert_runtime_error_message(&output, "runtime_error", "run 'missing-run' not found");
}

#[test]
fn auth_list_and_models_error_paths_work() {
    let harness = CliHarness::new();

    let list = harness.run(&["auth", "list"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("PROVIDER"));
    assert!(list_stdout.contains("STATUS"));

    let models = harness.run(&["auth", "models", "definitely-unknown-provider"]);
    assert!(!models.status.success());
    let models_stderr = stderr(&models);
    assert!(models_stderr.contains("Unknown provider: definitely-unknown-provider"));
    assert!(models_stderr.contains("Run `opengoose auth list`"));
}

#[test]
fn event_history_command_lists_persisted_events() {
    let harness = CliHarness::new();
    let store = EventStore::new(open_database(harness.home()));

    store
        .record(&AppEventKind::MessageReceived {
            session_key: SessionKey::new(Platform::Discord, "ops", "bridge"),
            author: "alice".into(),
            content: "hello".into(),
        })
        .unwrap();

    let output = harness.run(&[
        "event",
        "history",
        "--filter",
        "gateway:discord",
        "--since",
        "48h",
    ]);
    assert!(output.status.success());

    let text = stdout(&output);
    assert!(text.contains("message_received"));
    assert!(text.contains("discord:ns:ops:bridge"));
}

#[test]
fn json_output_supports_auth_and_errors() {
    let harness = CliHarness::new();

    let list = harness.run(&["--json", "auth", "list"]);
    assert!(list.status.success());
    let list_json = stdout_json(&list);
    assert_eq!(list_json["ok"], Value::Bool(true));
    assert!(
        list_json["providers"]
            .as_array()
            .is_some_and(|providers| !providers.is_empty())
    );

    let models = harness.run(&["--json", "auth", "models", "definitely-unknown-provider"]);
    assert!(!models.status.success());
    let models_json = stderr_json(&models);
    assert_eq!(models_json["ok"], Value::Bool(false));
    assert_eq!(models_json["error"]["kind"], Value::from("invalid_input"));
    assert!(
        models_json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown provider: definitely-unknown-provider")
    );
}

#[test]
fn completion_command_prints_shell_scripts() {
    let harness = CliHarness::new();

    let bash = harness.run(&["completion", "bash"]);
    assert!(bash.status.success());
    let bash_stdout = stdout(&bash);
    assert!(bash_stdout.contains("_opengoose()"));
    assert!(bash_stdout.contains("complete -F"));

    let invalid_json = harness.run(&["--json", "completion", "bash"]);
    assert!(!invalid_json.status.success());
    let invalid_json_stderr = stderr_json(&invalid_json);
    assert_eq!(
        invalid_json_stderr["error"]["kind"],
        Value::from("unsupported_output")
    );
}
