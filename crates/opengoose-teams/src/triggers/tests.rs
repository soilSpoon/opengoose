#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;

    use super::super::eval::*;
    use super::super::types::*;
    use super::super::watchers::*;

    #[test]
    fn test_trigger_type_roundtrip() {
        for name in TriggerType::all_names() {
            let tt = TriggerType::parse(name).unwrap();
            assert_eq!(tt.as_str(), *name);
        }
    }

    #[test]
    fn test_trigger_type_invalid() {
        assert!(TriggerType::parse("bogus").is_none());
    }

    #[test]
    fn test_matches_message_empty_condition() {
        // Empty condition matches everything
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
    fn test_validate_trigger_type() {
        assert!(validate_trigger_type("file_watch").is_ok());
        assert!(validate_trigger_type("message_received").is_ok());
        assert!(validate_trigger_type("webhook_received").is_ok());
        assert!(validate_trigger_type("schedule_complete").is_ok());
        assert!(validate_trigger_type("on_message").is_ok());
        assert!(validate_trigger_type("on_session_start").is_ok());
        assert!(validate_trigger_type("on_session_end").is_ok());
        assert!(validate_trigger_type("on_schedule").is_ok());
        assert!(validate_trigger_type("nope").is_err());
    }

    #[test]
    fn test_validate_trigger_type_error_message_includes_valid_types() {
        let err = validate_trigger_type("invalid").unwrap_err();
        assert!(err.contains("file_watch"), "error should list valid types");
        assert!(
            err.contains("message_received"),
            "error should list valid types"
        );
        assert!(err.contains("invalid"), "error should mention the bad type");
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_boundary() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // 3-byte UTF-8 char: should truncate at valid char boundary
        let text = "aaa\u{2603}bbb"; // snowman (3 bytes)
        let result = truncate(text, 4);
        assert_eq!(result, "aaa"); // can't fit the snowman in 4 bytes
    }

    #[test]
    fn test_trigger_type_all_names_complete() {
        let names = TriggerType::all_names();
        assert_eq!(names.len(), 8);
        // Every name should roundtrip
        for name in names {
            assert!(TriggerType::parse(name).is_some());
        }
    }

    #[test]
    fn test_message_condition_deserialize_default() {
        let cond: MessageCondition = serde_json::from_str("{}").unwrap();
        assert!(cond.from_agent.is_none());
        assert!(cond.channel.is_none());
        assert!(cond.payload_contains.is_none());
    }

    #[test]
    fn test_message_condition_serialize_skips_none() {
        let cond = MessageCondition {
            from_agent: Some("agent-a".into()),
            channel: None,
            payload_contains: None,
        };
        let json = serde_json::to_string(&cond).unwrap();
        assert!(json.contains("from_agent"));
        assert!(!json.contains("channel"));
        assert!(!json.contains("payload_contains"));
    }

    #[test]
    fn test_file_watch_condition_roundtrip() {
        let cond = FileWatchCondition {
            pattern: Some("src/**/*.rs".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: FileWatchCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pattern, Some("src/**/*.rs".into()));
    }

    #[test]
    fn test_webhook_condition_roundtrip() {
        let cond = WebhookCondition {
            path: Some("/github/pr".into()),
            secret: None,
            hmac_secret: Some("signing-secret".into()),
            signature_header: Some("X-Hub-Signature-256".into()),
            timestamp_header: Some("X-Hub-Timestamp".into()),
            timestamp_tolerance_secs: Some(120),
            rate_limit: None,
            rate_limit_window_secs: None,
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: WebhookCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, Some("/github/pr".into()));
        assert_eq!(parsed.hmac_secret, Some("signing-secret".into()));
        assert_eq!(parsed.signature_header, Some("X-Hub-Signature-256".into()));
        assert_eq!(parsed.timestamp_header, Some("X-Hub-Timestamp".into()));
        assert_eq!(parsed.timestamp_tolerance_secs, Some(120));
    }

    #[test]
    fn test_schedule_complete_condition_roundtrip() {
        let cond = ScheduleCompleteCondition {
            schedule_name: Some("nightly-build".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: ScheduleCompleteCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schedule_name, Some("nightly-build".into()));
    }

    #[test]
    fn test_trigger_type_serde_roundtrip() {
        for tt in [
            TriggerType::FileWatch,
            TriggerType::MessageReceived,
            TriggerType::ScheduleComplete,
            TriggerType::WebhookReceived,
            TriggerType::OnMessage,
            TriggerType::OnSessionStart,
            TriggerType::OnSessionEnd,
            TriggerType::OnSchedule,
        ] {
            let json = serde_json::to_string(&tt).unwrap();
            let parsed: TriggerType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, tt);
        }
    }

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

    // --- FileWatch matching tests ---

    #[test]
    fn test_matches_file_watch_no_pattern_matches_all() {
        // No `pattern` field → match everything.
        assert!(matches_file_watch_event("{}", "src/main.rs"));
        assert!(matches_file_watch_event("{}", "/tmp/foo.log"));
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
        // An unparseable glob pattern should not panic; it returns false.
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

    // --- WebhookPath matching tests ---

    #[test]
    fn test_matches_webhook_path_no_path_matches_all() {
        // No `path` field → every incoming path matches.
        assert!(matches_webhook_path("{}", "/github/pr"));
        assert!(matches_webhook_path("{}", "/any/path"));
        assert!(matches_webhook_path("{}", ""));
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
        // "/exactnot" also starts with "/exact" — this is expected prefix semantics.
        assert!(matches_webhook_path(cond, "/exactnot"));
        // A path that does NOT share the prefix must not match.
        assert!(!matches_webhook_path(cond, "/other"));
    }

    #[test]
    fn test_matches_webhook_path_invalid_json() {
        assert!(!matches_webhook_path("not json", "/any"));
        assert!(!matches_webhook_path("", "/any"));
    }

    #[test]
    fn test_matches_webhook_path_secret_field_ignored_by_path_check() {
        // `secret` field must not affect path matching.
        let cond = r#"{"path":"/hook","secret":"s3cr3t"}"#;
        assert!(matches_webhook_path(cond, "/hook/event"));
        assert!(!matches_webhook_path(cond, "/other"));
    }

    // --- Edge case tests for condition matching ---

    #[test]
    fn test_matches_message_empty_strings_match_any_condition() {
        // A condition with empty-string values should still filter correctly.
        let cond = r#"{"from_agent":"","channel":"","payload_contains":""}"#;
        // from_agent "" != "agent-a" → no match
        assert!(!matches_message_event(cond, "agent-a", Some("ch"), "msg"));
        // from_agent "" == "" → match on from, channel "" == "", payload contains "" → match
        assert!(matches_message_event(cond, "", Some(""), "anything"));
    }

    #[test]
    fn test_matches_on_message_empty_author_and_content() {
        let cond = r#"{"from_author":"","content_contains":""}"#;
        // from_author "" == "" and "" is in any string.
        assert!(matches_on_message_event(cond, "", ""));
        assert!(matches_on_message_event(cond, "", "anything"));
        // from_author "" != "alice" → no match
        assert!(!matches_on_message_event(cond, "alice", ""));
    }

    #[test]
    fn test_matches_on_session_empty_platform() {
        // A trigger filtering on empty-string platform only matches the empty string.
        let cond = r#"{"platform":""}"#;
        assert!(matches_on_session_event(cond, ""));
        assert!(!matches_on_session_event(cond, "discord"));
    }

    #[test]
    fn test_matches_on_schedule_empty_team() {
        let cond = r#"{"team":""}"#;
        assert!(matches_on_schedule_event(cond, ""));
        assert!(!matches_on_schedule_event(cond, "some-team"));
    }

    #[test]
    fn test_trigger_type_parse_empty_string() {
        assert!(TriggerType::parse("").is_none());
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
        assert_eq!(truncate("", 0), "");
    }

    #[test]
    fn test_matches_message_unknown_fields_ignored() {
        // Extra unknown fields in condition JSON must not affect matching.
        let cond = r#"{"from_agent":"bot","unknown_field":"ignored"}"#;
        assert!(matches_message_event(cond, "bot", None, "msg"));
        assert!(!matches_message_event(cond, "other", None, "msg"));
    }

    #[test]
    fn test_on_message_condition_serde_roundtrip() {
        let cond = OnMessageCondition {
            from_author: Some("alice".into()),
            content_contains: Some("hello".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: OnMessageCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_author, Some("alice".into()));
        assert_eq!(parsed.content_contains, Some("hello".into()));
    }

    #[test]
    fn test_on_session_condition_serde_roundtrip() {
        let cond = OnSessionCondition {
            platform: Some("discord".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: OnSessionCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.platform, Some("discord".into()));
    }

    #[test]
    fn test_on_schedule_condition_serde_roundtrip() {
        let cond = OnScheduleCondition {
            team: Some("nightly-review".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: OnScheduleCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.team, Some("nightly-review".into()));
    }

    #[test]
    fn test_matches_file_watch_empty_string_path() {
        // A valid glob matching empty string.
        let cond = r#"{"pattern":"*"}"#;
        assert!(matches_file_watch_event(cond, ""));
    }

    // --- Watcher lifecycle tests ---

    #[tokio::test]
    async fn test_file_watch_trigger_watcher_cancels_cleanly() {
        let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
        let event_bus = opengoose_types::EventBus::new(64);
        let cancel = CancellationToken::new();

        let handle = spawn_file_watch_trigger_watcher(db, event_bus, cancel.clone());

        // Give the task time to start up, then cancel it.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        // Should finish promptly after cancellation.
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("watcher did not stop within timeout")
            .expect("watcher task panicked");
    }

    #[tokio::test]
    async fn test_file_watch_trigger_fires_on_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
        let event_bus = opengoose_types::EventBus::new(64);
        let cancel = CancellationToken::new();

        // Register a file_watch trigger scoped to *.tmp files in the temp dir.
        let pattern = format!("{}/*.tmp", dir.path().display());
        let condition = serde_json::to_string(&FileWatchCondition {
            pattern: Some(pattern),
        })
        .unwrap();

        // The trigger references a non-existent team; the watcher will log a
        // warning but must not panic.
        opengoose_persistence::TriggerStore::new(db.clone())
            .create("watch-test", "file_watch", &condition, "no-such-team", "")
            .unwrap();

        // Change into the temp dir so the watcher root covers our test file.
        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).ok();

        let handle = spawn_file_watch_trigger_watcher(db, event_bus, cancel.clone());

        // Allow the watcher to initialise.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create a matching file — this should generate a notify event.
        let tmp_file = dir.path().join("test.tmp");
        std::fs::write(&tmp_file, b"hello").unwrap();

        // Give the event time to propagate.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("watcher did not stop within timeout")
            .expect("watcher task panicked");

        // Restore working directory.
        std::env::set_current_dir(prev_dir).ok();
    }
}
