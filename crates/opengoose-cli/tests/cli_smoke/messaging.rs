use crate::support::{CliHarness, assert_runtime_error_message, stdout};

#[test]
fn message_send_requires_destination() {
    let harness = CliHarness::new();

    let output = harness.run(&["--json", "message", "send", "--from", "frontend", "hello"]);
    assert_runtime_error_message(
        &output,
        "runtime_error",
        "specify either --to <agent> or --channel <name>",
    );
}

#[test]
fn message_send_rejects_to_and_channel_together() {
    let harness = CliHarness::new();

    let output = harness.run(&[
        "--json",
        "message",
        "send",
        "--from",
        "frontend",
        "--to",
        "backend",
        "--channel",
        "ops",
        "hello",
    ]);
    assert_runtime_error_message(
        &output,
        "runtime_error",
        "specify either --to or --channel, not both",
    );
}

#[test]
fn message_list_empty_session_reports_no_messages() {
    let harness = CliHarness::new();

    let output = harness.run(&["message", "list"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("No messages found."));
}

#[test]
fn message_pending_empty_session_reports_no_messages() {
    let harness = CliHarness::new();

    let output = harness.run(&["message", "pending", "backend"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("No pending messages for 'backend'."));
}

#[test]
fn message_directed_round_trip_lists_and_receives_pending_messages() {
    let harness = CliHarness::new();

    let send = harness.run(&[
        "message",
        "send",
        "--from",
        "frontend",
        "--to",
        "backend",
        "please review",
    ]);
    assert!(send.status.success());
    assert!(stdout(&send).contains("Directed message sent"));

    let list = harness.run(&["message", "list", "--agent", "backend"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("frontend"));
    assert!(list_stdout.contains("backend"));
    assert!(list_stdout.contains("directed"));
    assert!(list_stdout.contains("please review"));

    let pending = harness.run(&["message", "pending", "backend"]);
    assert!(pending.status.success());
    let pending_stdout = stdout(&pending);
    assert!(pending_stdout.contains("Pending messages for 'backend':"));
    assert!(pending_stdout.contains("frontend"));
    assert!(pending_stdout.contains("please review"));
}

#[test]
fn message_channel_round_trip_lists_history() {
    let harness = CliHarness::new();

    let send = harness.run(&[
        "message",
        "send",
        "--from",
        "frontend",
        "--channel",
        "ops",
        "channel hello",
    ]);
    assert!(send.status.success());
    assert!(stdout(&send).contains("Channel message published"));

    let list = harness.run(&["message", "list", "--channel", "ops"]);
    assert!(list.status.success());
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains("frontend"));
    assert!(list_stdout.contains("ops"));
    assert!(list_stdout.contains("channel"));
    assert!(list_stdout.contains("channel hello"));
}
