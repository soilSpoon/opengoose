use std::process::{Command, Output};

use assert_cmd::prelude::CommandCargoExt;
use tempfile::TempDir;

fn new_home() -> TempDir {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("goose")).unwrap();
    home
}

#[allow(deprecated)]
fn build_cmd(home: &TempDir, args: &[&str]) -> Command {
    let goose_root = home.path().join("goose");
    let mut cmd = Command::cargo_bin("opengoose").unwrap();
    cmd.args(args);
    cmd.env("HOME", home.path())
        .env("GOOSE_PATH_ROOT", &goose_root)
        .env("GOOSE_DISABLE_KEYRING", "1");
    cmd
}

fn render_output(output: &Output) -> String {
    format!(
        "status={:?}\nstdout=\n{}\nstderr=\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run_success(home: &TempDir, args: &[&str]) -> String {
    let output = build_cmd(home, args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: args={args:?}\n{}",
        render_output(&output)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_failure(home: &TempDir, args: &[&str]) -> String {
    let output = build_cmd(home, args).output().unwrap();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: args={args:?}\n{}",
        render_output(&output)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_in_order(haystack: &str, needles: &[&str]) {
    let mut offset = 0;
    for needle in needles {
        let idx = haystack[offset..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing substring `{needle}` in:\n{haystack}"));
        offset += idx + needle.len();
    }
}

#[test]
fn message_send_directed_prints_summary() {
    let home = new_home();
    let stdout = run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "frontend",
            "--to",
            "backend",
            "hello backend",
        ],
    );

    assert!(stdout.contains("Directed message sent"));
    assert!(stdout.contains("From:    frontend"));
    assert!(stdout.contains("To:      backend"));
    assert!(stdout.contains("Payload: hello backend"));
}

#[test]
fn message_send_channel_prints_summary() {
    let home = new_home();
    let stdout = run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "frontend",
            "--channel",
            "triage",
            "hello channel",
        ],
    );

    assert!(stdout.contains("Channel message published"));
    assert!(stdout.contains("From:    frontend"));
    assert!(stdout.contains("Channel: triage"));
    assert!(stdout.contains("Payload: hello channel"));
}

#[test]
fn message_list_recent_shows_messages_oldest_first() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
            "first payload",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "agent-c",
            "--channel",
            "ops",
            "second payload",
        ],
    );

    let stdout = run_success(&home, &["message", "list"]);

    assert!(stdout.contains("PAYLOAD"));
    assert!(stdout.contains("2 message(s)."));
    assert_in_order(&stdout, &["first payload", "second payload"]);
}

#[test]
fn message_list_recent_truncates_long_payload_preview() {
    let home = new_home();
    let payload = "1234567890123456789012345678901234567890EXTRA";
    run_success(
        &home,
        &[
            "message", "send", "--from", "agent-a", "--to", "agent-b", payload,
        ],
    );

    let stdout = run_success(&home, &["message", "list"]);

    assert!(stdout.contains("123456789012345678901234567890123456789"));
    assert!(!stdout.contains(payload));
}

#[test]
fn message_list_agent_filter_shows_incoming_and_outgoing_messages() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message", "send", "--from", "alice", "--to", "bob", "for bob",
        ],
    );
    run_success(
        &home,
        &[
            "message", "send", "--from", "bob", "--to", "carol", "from bob",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--to",
            "carol",
            "not for bob",
        ],
    );

    let stdout = run_success(
        &home,
        &["message", "list", "--agent", "bob", "--limit", "10"],
    );

    assert!(stdout.contains("2 message(s)."));
    assert!(stdout.contains("for bob"));
    assert!(stdout.contains("from bob"));
    assert!(!stdout.contains("not for bob"));
    assert_in_order(&stdout, &["for bob", "from bob"]);
}

#[test]
fn message_list_channel_filter_shows_only_requested_channel() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "general",
            "general update",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "random",
            "random update",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--to",
            "bob",
            "direct update",
        ],
    );

    let stdout = run_success(&home, &["message", "list", "--channel", "general"]);

    assert!(stdout.contains("1 message(s)."));
    assert!(stdout.contains("general update"));
    assert!(!stdout.contains("random update"));
    assert!(!stdout.contains("direct update"));
}

#[test]
fn message_list_channel_filter_respects_limit() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "general",
            "first",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "general",
            "second",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "general",
            "third",
        ],
    );

    let stdout = run_success(
        &home,
        &["message", "list", "--channel", "general", "--limit", "2"],
    );

    assert!(stdout.contains("2 message(s)."));
    assert!(!stdout.contains("first"));
    assert_in_order(&stdout, &["second", "third"]);
}

#[test]
fn message_list_uses_session_filter() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message",
            "send",
            "--session",
            "cli:test:alpha",
            "--from",
            "alice",
            "--to",
            "bob",
            "alpha message",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--session",
            "cli:test:beta",
            "--from",
            "alice",
            "--to",
            "bob",
            "beta message",
        ],
    );

    let stdout = run_success(
        &home,
        &[
            "message",
            "list",
            "--session",
            "cli:test:beta",
            "--limit",
            "10",
        ],
    );

    assert!(stdout.contains("1 message(s)."));
    assert!(stdout.contains("beta message"));
    assert!(!stdout.contains("alpha message"));
}

#[test]
fn message_pending_shows_directed_messages_for_agent_only() {
    let home = new_home();
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--to",
            "bob",
            "hello bob",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--to",
            "carol",
            "hello carol",
        ],
    );
    run_success(
        &home,
        &[
            "message",
            "send",
            "--from",
            "alice",
            "--channel",
            "general",
            "broadcast update",
        ],
    );

    let stdout = run_success(&home, &["message", "pending", "bob"]);

    assert!(stdout.contains("Pending messages for 'bob':"));
    assert!(stdout.contains("hello bob"));
    assert!(!stdout.contains("hello carol"));
    assert!(!stdout.contains("broadcast update"));
    assert!(stdout.contains("1 pending message(s)."));
}

#[test]
fn message_pending_truncates_long_payload_preview() {
    let home = new_home();
    let payload = "abcdefghijklmnopqrstuvwxyz0123456789LONGER";
    run_success(
        &home,
        &["message", "send", "--from", "alice", "--to", "bob", payload],
    );

    let stdout = run_success(&home, &["message", "pending", "bob"]);

    assert!(stdout.contains("abcdefghijklmnopqrstuvwxyz0123456789LON"));
    assert!(!stdout.contains(payload));
}

#[test]
fn message_pending_reports_empty_state() {
    let home = new_home();
    let stdout = run_success(&home, &["message", "pending", "nobody"]);

    assert!(stdout.contains("No pending messages for 'nobody'."));
}

#[test]
fn message_list_reports_empty_state() {
    let home = new_home();
    let stdout = run_success(&home, &["message", "list", "--session", "cli:test:empty"]);

    assert!(stdout.contains("No messages found."));
}

#[test]
fn message_send_rejects_both_to_and_channel() {
    let home = new_home();
    let stderr = run_failure(
        &home,
        &[
            "message",
            "send",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
            "--channel",
            "general",
            "hello",
        ],
    );

    assert!(stderr.contains("specify either --to or --channel, not both"));
}

#[test]
fn message_list_rejects_non_positive_limit() {
    let home = new_home();
    let stderr = run_failure(&home, &["message", "list", "--limit", "0"]);

    assert!(stderr.contains("--limit must be at least 1"));
}

#[test]
fn message_list_rejects_agent_and_channel_together() {
    let home = new_home();
    let stderr = run_failure(
        &home,
        &["message", "list", "--agent", "bob", "--channel", "general"],
    );

    assert!(stderr.contains("specify either --agent or --channel, not both"));
}

#[test]
fn message_subscribe_channel_timeout_prints_banner_and_timeout() {
    let home = new_home();
    let stdout = run_success(
        &home,
        &["message", "subscribe", "--channel", "ops", "--timeout", "1"],
    );

    assert!(stdout.contains("Subscribed to channel 'ops'"));
    assert!(stdout.contains("Subscription timeout."));
}

#[test]
fn message_subscribe_agent_timeout_prints_banner_and_timeout() {
    let home = new_home();
    let stdout = run_success(
        &home,
        &[
            "message",
            "subscribe",
            "--agent",
            "backend",
            "--timeout",
            "1",
        ],
    );

    assert!(stdout.contains("Subscribed to directed messages for 'backend'"));
    assert!(stdout.contains("Subscription timeout."));
}
