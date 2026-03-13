use std::sync::Arc;

use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::EventBus;

use crate::message_bus::BusEvent;
use crate::triggers::handlers::{handle_app_event, handle_bus_event, truncate};

/// Helper: create an in-memory DB wrapped in an Arc.
fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

/// Helper: create a minimal `BusEvent`.
fn bus_event(from: &str, channel: Option<&str>, payload: &str) -> BusEvent {
    BusEvent {
        from: from.to_string(),
        to: None,
        channel: channel.map(String::from),
        payload: payload.to_string(),
        timestamp: 0,
    }
}

// ── truncate ────────────────────────────────────────────────────────

#[test]
fn truncate_short_string_unchanged() {
    assert_eq!(truncate("hello", 10), "hello");
}

#[test]
fn truncate_exact_length_unchanged() {
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn truncate_long_string_trimmed() {
    let result = truncate("hello world", 5);
    assert_eq!(result, "hello");
}

#[test]
fn truncate_empty_string() {
    assert_eq!(truncate("", 5), "");
}

#[test]
fn truncate_respects_char_boundaries() {
    // "café" is 5 bytes (é is 2 bytes). Truncating at 4 should not split the é.
    let result = truncate("café", 4);
    assert_eq!(result, "caf");
}

#[test]
fn truncate_zero_max() {
    assert_eq!(truncate("hello", 0), "");
}

// ── handle_bus_event (message_received triggers) ────────────────────

#[tokio::test]
async fn bus_event_no_triggers_returns_ok() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let event = bus_event("agent-a", None, "hello");

    let result = handle_bus_event(&db, &event_bus, &event).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn bus_event_non_matching_trigger_not_fired() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "t1",
            "message_received",
            r#"{"from_agent":"bot-x"}"#,
            "team-a",
            "",
        )
        .unwrap();

    let event = bus_event("agent-a", None, "hello");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let t = store.get_by_name("t1").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn bus_event_matching_trigger_fires_and_marks() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "t1",
            "message_received",
            r#"{"from_agent":"agent-a"}"#,
            "nonexistent-team",
            "custom input",
        )
        .unwrap();

    let event = bus_event("agent-a", None, "payload");
    // run_headless will fail because team doesn't exist, but the trigger
    // still gets marked as fired.
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let t = store.get_by_name("t1").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
    assert!(t.last_fired_at.is_some());
}

#[tokio::test]
async fn bus_event_empty_condition_matches_all() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("t-all", "message_received", "{}", "no-team", "")
        .unwrap();

    let event = bus_event("any-sender", Some("any-chan"), "any payload");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let t = store.get_by_name("t-all").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn bus_event_channel_match() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "chan-trig",
            "message_received",
            r#"{"channel":"alerts"}"#,
            "no-team",
            "",
        )
        .unwrap();

    // Non-matching channel
    let event = bus_event("a", Some("general"), "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("chan-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);

    // Matching channel
    let event = bus_event("a", Some("alerts"), "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("chan-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn bus_event_no_channel_does_not_match_channel_filter() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "chan-only",
            "message_received",
            r#"{"channel":"alerts"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let event = bus_event("a", None, "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("chan-only").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn bus_event_payload_contains_match() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "kw-trig",
            "message_received",
            r#"{"payload_contains":"deploy"}"#,
            "no-team",
            "",
        )
        .unwrap();

    // Non-matching payload
    let event = bus_event("a", None, "running tests");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("kw-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);

    // Matching payload
    let event = bus_event("a", None, "starting deploy now");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("kw-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn bus_event_disabled_trigger_not_evaluated() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("disabled", "message_received", "{}", "no-team", "")
        .unwrap();
    store.set_enabled("disabled", false).unwrap();

    let event = bus_event("a", None, "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();
    let t = store.get_by_name("disabled").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn bus_event_empty_input_generates_fallback() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("fallback", "message_received", "{}", "no-team", "")
        .unwrap();

    let event = bus_event("sender-x", None, "some payload");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    // Trigger fired (mark_fired was called) even though run_headless fails.
    let t = store.get_by_name("fallback").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

// ── handle_app_event (on_message, on_session_start, etc.) ──────────

#[tokio::test]
async fn app_event_no_triggers_returns_ok() {
    let db = test_db();
    let event_bus = EventBus::new(16);

    let kind = opengoose_types::AppEventKind::GooseReady;
    let result = handle_app_event(&db, &event_bus, &kind).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn app_event_goose_ready_fires_session_start_trigger() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "ready-trig",
            "on_session_start",
            r#"{"platform":"system"}"#,
            "no-team",
            "session started",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::GooseReady;
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("ready-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn app_event_goose_ready_does_not_fire_wrong_platform() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "discord-only",
            "on_session_start",
            r#"{"platform":"discord"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::GooseReady;
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("discord-only").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn app_event_channel_ready_fires_session_start() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "discord-ready",
            "on_session_start",
            r#"{"platform":"discord"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::ChannelReady {
        platform: opengoose_types::Platform::Discord,
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("discord-ready").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn app_event_channel_ready_wrong_platform_no_fire() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "slack-only",
            "on_session_start",
            r#"{"platform":"slack"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::ChannelReady {
        platform: opengoose_types::Platform::Discord,
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("slack-only").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn app_event_session_disconnected_fires_session_end() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "disc-end",
            "on_session_end",
            r#"{"platform":"telegram"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::SessionDisconnected {
        session_key: opengoose_types::SessionKey {
            platform: opengoose_types::Platform::Telegram,
            namespace: None,
            channel_id: "ch-1".to_string(),
        },
        reason: "timeout".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("disc-end").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn app_event_message_received_fires_on_message() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "on-msg",
            "on_message",
            r#"{"from_author":"alice"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::MessageReceived {
        session_key: opengoose_types::SessionKey {
            platform: opengoose_types::Platform::Discord,
            namespace: None,
            channel_id: "ch".to_string(),
        },
        author: "alice".to_string(),
        content: "hello".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("on-msg").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn app_event_message_received_wrong_author_no_fire() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "alice-only",
            "on_message",
            r#"{"from_author":"alice"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::MessageReceived {
        session_key: opengoose_types::SessionKey {
            platform: opengoose_types::Platform::Discord,
            namespace: None,
            channel_id: "ch".to_string(),
        },
        author: "bob".to_string(),
        content: "hi".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("alice-only").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn app_event_team_run_completed_fires_on_schedule() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "sched-trig",
            "on_schedule",
            r#"{"team":"nightly-build"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::TeamRunCompleted {
        team: "nightly-build".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("sched-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn app_event_team_run_completed_wrong_team_no_fire() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "wrong-team",
            "on_schedule",
            r#"{"team":"nightly-build"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::TeamRunCompleted {
        team: "daily-cleanup".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("wrong-team").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn app_event_unhandled_variant_returns_ok() {
    let db = test_db();
    let event_bus = EventBus::new(16);

    let kind = opengoose_types::AppEventKind::DashboardUpdated;
    let result = handle_app_event(&db, &event_bus, &kind).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn app_event_empty_condition_matches_any_platform() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("any-start", "on_session_start", "{}", "no-team", "")
        .unwrap();

    let kind = opengoose_types::AppEventKind::ChannelReady {
        platform: opengoose_types::Platform::Slack,
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("any-start").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

// ── fire_file_watch_triggers ────────────────────────────────────────

#[tokio::test]
async fn file_watch_no_triggers_returns_ok() {
    let db = test_db();
    let event_bus = EventBus::new(16);

    let result = crate::triggers::fire_file_watch_triggers(&db, &event_bus, "/some/path.rs").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn file_watch_matching_glob_fires() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "rs-watch",
            "file_watch",
            r#"{"pattern":"**/*.rs"}"#,
            "no-team",
            "",
        )
        .unwrap();

    crate::triggers::fire_file_watch_triggers(&db, &event_bus, "src/main.rs")
        .await
        .unwrap();

    let t = store.get_by_name("rs-watch").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn file_watch_non_matching_glob_no_fire() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "rs-only",
            "file_watch",
            r#"{"pattern":"**/*.rs"}"#,
            "no-team",
            "",
        )
        .unwrap();

    crate::triggers::fire_file_watch_triggers(&db, &event_bus, "src/main.py")
        .await
        .unwrap();

    let t = store.get_by_name("rs-only").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn file_watch_empty_condition_matches_all() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("catch-all", "file_watch", "{}", "no-team", "")
        .unwrap();

    crate::triggers::fire_file_watch_triggers(&db, &event_bus, "anything.txt")
        .await
        .unwrap();

    let t = store.get_by_name("catch-all").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

// ── multiple triggers ───────────────────────────────────────────────

#[tokio::test]
async fn multiple_matching_triggers_all_fire() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("t1", "message_received", "{}", "no-team", "")
        .unwrap();
    store
        .create("t2", "message_received", "{}", "no-team", "")
        .unwrap();

    let event = bus_event("a", None, "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let t1 = store.get_by_name("t1").unwrap().unwrap();
    let t2 = store.get_by_name("t2").unwrap().unwrap();
    assert_eq!(t1.fire_count, 1);
    assert_eq!(t2.fire_count, 1);
}

#[tokio::test]
async fn mixed_matching_and_non_matching_triggers() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("match-me", "message_received", "{}", "no-team", "")
        .unwrap();
    store
        .create(
            "skip-me",
            "message_received",
            r#"{"from_agent":"other"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let event = bus_event("agent-a", None, "hello");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let matched = store.get_by_name("match-me").unwrap().unwrap();
    let skipped = store.get_by_name("skip-me").unwrap().unwrap();
    assert_eq!(matched.fire_count, 1);
    assert_eq!(skipped.fire_count, 0);
}

// ── on_message content_contains ─────────────────────────────────────

#[tokio::test]
async fn on_message_content_contains_match() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "kw-msg",
            "on_message",
            r#"{"content_contains":"urgent"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::MessageReceived {
        session_key: opengoose_types::SessionKey {
            platform: opengoose_types::Platform::Slack,
            namespace: None,
            channel_id: "ch".to_string(),
        },
        author: "anyone".to_string(),
        content: "this is urgent please help".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("kw-msg").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

#[tokio::test]
async fn on_message_content_contains_no_match() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create(
            "kw-msg2",
            "on_message",
            r#"{"content_contains":"urgent"}"#,
            "no-team",
            "",
        )
        .unwrap();

    let kind = opengoose_types::AppEventKind::MessageReceived {
        session_key: opengoose_types::SessionKey {
            platform: opengoose_types::Platform::Slack,
            namespace: None,
            channel_id: "ch".to_string(),
        },
        author: "anyone".to_string(),
        content: "everything is fine".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("kw-msg2").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

// ── on_schedule with empty condition ────────────────────────────────

#[tokio::test]
async fn on_schedule_empty_condition_matches_any_team() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("any-sched", "on_schedule", "{}", "no-team", "")
        .unwrap();

    let kind = opengoose_types::AppEventKind::TeamRunCompleted {
        team: "arbitrary-team".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("any-sched").unwrap().unwrap();
    assert_eq!(t.fire_count, 1);
}

// ── cross-type isolation ────────────────────────────────────────────

#[tokio::test]
async fn file_watch_trigger_not_fired_by_message_event() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("fw-trig", "file_watch", "{}", "no-team", "")
        .unwrap();

    let event = bus_event("a", None, "msg");
    handle_bus_event(&db, &event_bus, &event).await.unwrap();

    let t = store.get_by_name("fw-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn message_received_trigger_not_fired_by_file_watch() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("msg-trig", "message_received", "{}", "no-team", "")
        .unwrap();

    crate::triggers::fire_file_watch_triggers(&db, &event_bus, "any/file.txt")
        .await
        .unwrap();

    let t = store.get_by_name("msg-trig").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}

#[tokio::test]
async fn on_session_start_not_fired_by_on_schedule_event() {
    let db = test_db();
    let event_bus = EventBus::new(16);
    let store = TriggerStore::new(db.clone());

    store
        .create("sess-start", "on_session_start", "{}", "no-team", "")
        .unwrap();

    let kind = opengoose_types::AppEventKind::TeamRunCompleted {
        team: "any".to_string(),
    };
    handle_app_event(&db, &event_bus, &kind).await.unwrap();

    let t = store.get_by_name("sess-start").unwrap().unwrap();
    assert_eq!(t.fire_count, 0);
}
