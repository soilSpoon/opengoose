use super::super::*;

// --- Message matching tests ---

#[test]
fn test_matches_message_empty_condition() {
    assert!(matches_message_event("{}", "agent-a", Some("ch"), "hello"));
}

#[test]
fn test_matches_message_from_filter() {
    let cond = r#"{"from_agent":"agent-a"}"#;
    assert!(matches_message_event(cond, "agent-a", None, "msg"));
    assert!(!matches_message_event(cond, "agent-b", None, "msg"));
}

#[test]
fn test_matches_message_channel_filter() {
    let cond = r#"{"channel":"alerts"}"#;
    assert!(matches_message_event(cond, "any", Some("alerts"), "msg"));
    assert!(!matches_message_event(cond, "any", Some("other"), "msg"));
    assert!(!matches_message_event(cond, "any", None, "msg"));
}

#[test]
fn test_matches_message_payload_contains() {
    let cond = r#"{"payload_contains":"ERROR"}"#;
    assert!(matches_message_event(
        cond,
        "any",
        None,
        "got an ERROR here"
    ));
    assert!(!matches_message_event(cond, "any", None, "all good"));
}

#[test]
fn test_matches_message_empty_payload_contains_matches_any_payload() {
    let cond = r#"{"payload_contains":""}"#;
    assert!(matches_message_event(cond, "any", None, ""));
    assert!(matches_message_event(
        cond,
        "any",
        Some("alerts"),
        "all good"
    ));
}

#[test]
fn test_matches_message_combined() {
    let cond = r#"{"from_agent":"monitor","channel":"alerts","payload_contains":"critical"}"#;
    assert!(matches_message_event(
        cond,
        "monitor",
        Some("alerts"),
        "critical failure"
    ));
    assert!(!matches_message_event(
        cond,
        "other",
        Some("alerts"),
        "critical failure"
    ));
    assert!(!matches_message_event(
        cond,
        "monitor",
        Some("alerts"),
        "minor issue"
    ));
}

#[test]
fn test_matches_message_invalid_json() {
    assert!(!matches_message_event("not json", "a", None, "b"));
}

#[test]
fn test_matches_message_empty_strings_match_any_condition() {
    let cond = r#"{"from_agent":"","channel":"","payload_contains":""}"#;
    assert!(!matches_message_event(cond, "agent-a", Some("ch"), "msg"));
    assert!(matches_message_event(cond, "", Some(""), "anything"));
}

#[test]
fn test_matches_message_unknown_fields_ignored() {
    let cond = r#"{"from_agent":"bot","unknown_field":"ignored"}"#;
    assert!(matches_message_event(cond, "bot", None, "msg"));
    assert!(!matches_message_event(cond, "other", None, "msg"));
}

// --- OnMessage matching tests ---

#[test]
fn test_matches_on_message_empty_condition() {
    assert!(matches_on_message_event("{}", "alice", "hello world"));
}

#[test]
fn test_matches_on_message_from_author_filter() {
    let cond = r#"{"from_author":"alice"}"#;
    assert!(matches_on_message_event(cond, "alice", "msg"));
    assert!(!matches_on_message_event(cond, "bob", "msg"));
}

#[test]
fn test_matches_on_message_content_contains_filter() {
    let cond = r#"{"content_contains":"alert"}"#;
    assert!(matches_on_message_event(cond, "any", "critical alert!"));
    assert!(!matches_on_message_event(cond, "any", "all good"));
}

#[test]
fn test_matches_on_message_empty_content_filter_matches_any_content() {
    let cond = r#"{"content_contains":""}"#;
    assert!(matches_on_message_event(cond, "alice", ""));
    assert!(matches_on_message_event(cond, "alice", "all good"));
}

#[test]
fn test_matches_on_message_combined() {
    let cond = r#"{"from_author":"monitor","content_contains":"error"}"#;
    assert!(matches_on_message_event(cond, "monitor", "error detected"));
    assert!(!matches_on_message_event(cond, "other", "error detected"));
    assert!(!matches_on_message_event(cond, "monitor", "all clear"));
}

#[test]
fn test_matches_on_message_invalid_json() {
    assert!(!matches_on_message_event("not json", "a", "b"));
}

#[test]
fn test_matches_on_message_empty_author_and_content() {
    let cond = r#"{"from_author":"","content_contains":""}"#;
    assert!(matches_on_message_event(cond, "", ""));
    assert!(matches_on_message_event(cond, "", "anything"));
    assert!(!matches_on_message_event(cond, "alice", ""));
}

// --- OnSession matching tests ---

#[test]
fn test_matches_on_session_empty_condition() {
    assert!(matches_on_session_event("{}", "discord"));
    assert!(matches_on_session_event("{}", "system"));
}

#[test]
fn test_matches_on_session_platform_filter() {
    let cond = r#"{"platform":"discord"}"#;
    assert!(matches_on_session_event(cond, "discord"));
    assert!(!matches_on_session_event(cond, "slack"));
}

#[test]
fn test_matches_on_session_invalid_json() {
    assert!(!matches_on_session_event("not json", "discord"));
}

#[test]
fn test_matches_on_session_empty_platform() {
    let cond = r#"{"platform":""}"#;
    assert!(matches_on_session_event(cond, ""));
    assert!(!matches_on_session_event(cond, "discord"));
}

// --- OnSchedule matching tests ---

#[test]
fn test_matches_on_schedule_empty_condition() {
    assert!(matches_on_schedule_event("{}", "any-team"));
}

#[test]
fn test_matches_on_schedule_team_filter() {
    let cond = r#"{"team":"code-review"}"#;
    assert!(matches_on_schedule_event(cond, "code-review"));
    assert!(!matches_on_schedule_event(cond, "bug-triage"));
}

#[test]
fn test_matches_on_schedule_invalid_json() {
    assert!(!matches_on_schedule_event("not json", "team"));
}

#[test]
fn test_matches_on_schedule_empty_team() {
    let cond = r#"{"team":""}"#;
    assert!(matches_on_schedule_event(cond, ""));
    assert!(!matches_on_schedule_event(cond, "some-team"));
}

// --- FileWatch matching tests ---

#[test]
fn test_matches_file_watch_no_pattern_matches_all() {
    assert!(matches_file_watch_event("{}", "src/main.rs"));
    assert!(matches_file_watch_event("{}", "/tmp/foo.log"));
}

#[test]
fn test_matches_file_watch_empty_pattern_matches_all() {
    let cond = r#"{"pattern":""}"#;
    assert!(matches_file_watch_event(cond, "src/main.rs"));
    assert!(matches_file_watch_event(cond, "/tmp/foo.log"));
    assert!(matches_file_watch_event(cond, ""));
}

#[test]
fn test_matches_file_watch_simple_glob() {
    let cond = r#"{"pattern":"src/**/*.rs"}"#;
    assert!(matches_file_watch_event(cond, "src/lib.rs"));
    assert!(matches_file_watch_event(cond, "src/foo/bar.rs"));
    assert!(!matches_file_watch_event(cond, "tests/foo.rs"));
    assert!(!matches_file_watch_event(cond, "src/foo.txt"));
}

#[test]
fn test_matches_file_watch_extension_glob() {
    let cond = r#"{"pattern":"**/*.log"}"#;
    assert!(matches_file_watch_event(cond, "var/log/app.log"));
    assert!(matches_file_watch_event(cond, "app.log"));
    assert!(!matches_file_watch_event(cond, "app.txt"));
}

#[test]
fn test_matches_file_watch_exact_path() {
    let cond = r#"{"pattern":"config.toml"}"#;
    assert!(matches_file_watch_event(cond, "config.toml"));
    assert!(!matches_file_watch_event(cond, "other.toml"));
}

#[test]
fn test_matches_file_watch_invalid_json() {
    assert!(!matches_file_watch_event("not json", "src/main.rs"));
}

#[test]
fn test_matches_file_watch_invalid_glob() {
    let cond = r#"{"pattern":"["}"#;
    assert!(!matches_file_watch_event(cond, "anything"));
}

#[test]
fn test_matches_file_watch_condition_roundtrip() {
    let cond = FileWatchCondition {
        pattern: Some("data/**/*.csv".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    assert!(matches_file_watch_event(&json, "data/2024/sales.csv"));
    assert!(!matches_file_watch_event(&json, "data/2024/sales.json"));
}

#[test]
fn test_matches_file_watch_empty_string_path() {
    let cond = r#"{"pattern":"*"}"#;
    assert!(matches_file_watch_event(cond, ""));
}

// --- WebhookPath matching tests ---

#[test]
fn test_matches_webhook_path_no_path_matches_all() {
    assert!(matches_webhook_path("{}", "/github/pr"));
    assert!(matches_webhook_path("{}", "/any/path"));
    assert!(matches_webhook_path("{}", ""));
}

#[test]
fn test_matches_webhook_path_empty_prefix_matches_all() {
    let cond = r#"{"path":""}"#;
    assert!(matches_webhook_path(cond, "/github/pr"));
    assert!(matches_webhook_path(cond, "/any/path"));
    assert!(matches_webhook_path(cond, ""));
}

#[test]
fn test_matches_webhook_path_prefix_match() {
    let cond = r#"{"path":"/github"}"#;
    assert!(matches_webhook_path(cond, "/github/pr"));
    assert!(matches_webhook_path(cond, "/github/push"));
    assert!(matches_webhook_path(cond, "/github"));
}

#[test]
fn test_matches_webhook_path_no_match() {
    let cond = r#"{"path":"/github"}"#;
    assert!(!matches_webhook_path(cond, "/gitlab/pr"));
    assert!(!matches_webhook_path(cond, "/git/hub"));
    assert!(!matches_webhook_path(cond, ""));
}

#[test]
fn test_matches_webhook_path_exact_path() {
    let cond = r#"{"path":"/exact"}"#;
    assert!(matches_webhook_path(cond, "/exact"));
    assert!(matches_webhook_path(cond, "/exact/sub")); // prefix match
    assert!(matches_webhook_path(cond, "/exactnot"));
    assert!(!matches_webhook_path(cond, "/other"));
}

#[test]
fn test_matches_webhook_path_invalid_json() {
    assert!(!matches_webhook_path("not json", "/any"));
    assert!(!matches_webhook_path("", "/any"));
}

#[test]
fn test_matches_webhook_path_secret_field_ignored_by_path_check() {
    let cond = r#"{"path":"/hook","secret":"s3cr3t"}"#;
    assert!(matches_webhook_path(cond, "/hook/event"));
    assert!(!matches_webhook_path(cond, "/other"));
}
