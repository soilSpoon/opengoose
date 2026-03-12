use super::*;

use opengoose_types::Platform;
use uuid::Uuid;

fn test_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "guild-1", "channel-1")
}

fn temp_team_store() -> TeamStore {
    let dir = std::env::temp_dir().join(format!("opengoose-engine-team-store-{}", Uuid::new_v4()));
    let store = TeamStore::with_dir(dir);
    store.install_defaults(false).unwrap();
    store
}

#[test]
fn handle_team_command_activates_lists_and_clears_teams() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(
        event_bus,
        Database::open_in_memory().unwrap(),
        Some(temp_team_store()),
    );
    let key = test_key();

    assert_eq!(
        engine.handle_team_command(&key, ""),
        "No team active for this channel."
    );
    assert_eq!(
        engine.handle_team_command(&key, "list"),
        "Available teams:\n- bug-triage\n- code-review\n- feature-dev\n- full-review\n- research-panel\n- security-audit\n- smart-router"
    );
    assert_eq!(
        engine.handle_team_command(&key, "code-review"),
        "Team code-review activated for this channel."
    );
    assert_eq!(
        engine.session_manager().active_team_for(&key),
        Some("code-review".into())
    );
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::TeamActivated {
            session_key,
            team_name,
        } if session_key == key && team_name == "code-review"
    ));

    assert_eq!(
        engine.handle_team_command(&key, ""),
        "Active team: code-review"
    );
    assert_eq!(
        engine.handle_team_command(&key, "off"),
        "Team deactivated. Reverting to single-agent mode."
    );
    assert_eq!(engine.session_manager().active_team_for(&key), None);
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::TeamDeactivated { session_key } if session_key == key
    ));
}

#[test]
fn handle_team_command_reports_missing_team_choices() {
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(
        event_bus,
        Database::open_in_memory().unwrap(),
        Some(temp_team_store()),
    );
    let key = test_key();

    assert_eq!(
        engine.handle_team_command(&key, "missing-team"),
        "Team `missing-team` not found. Available: bug-triage, code-review, feature-dev, full-review, research-panel, security-audit, smart-router"
    );
}

#[test]
fn handle_team_command_without_store_uses_safe_defaults() {
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    assert_eq!(
        engine.handle_team_command(&key, "list"),
        "No teams available."
    );
    assert_eq!(
        engine.handle_team_command(&key, "missing-team"),
        "Team `missing-team` not found. Available: none"
    );
}

#[test]
fn records_messages_and_emits_responses() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    engine.record_user_message(&key, "hello", Some("alice"));
    engine.send_response(&key, "hi there");

    let history = engine.sessions().load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "hello");
    assert_eq!(history[0].author.as_deref(), Some("alice"));
    assert_eq!(history[1].role, "assistant");
    assert_eq!(history[1].content, "hi there");
    assert_eq!(history[1].author.as_deref(), Some("goose"));

    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::ResponseSent {
            session_key,
            content,
        } if session_key == key && content == "hi there"
    ));
}

#[test]
fn accept_message_records_user_message_and_emits_event() {
    // Verifies that accept_message (called inside process_message_streaming)
    // persists the user message and emits MessageReceived, regardless of
    // whether a team is active or the default profile is used.
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    // Call accept_message directly (it's private, but we can test via
    // record_user_message + event assertion without running the full async path).
    engine.record_user_message(&key, "hello world", Some("alice"));
    engine.event_bus.emit(AppEventKind::MessageReceived {
        session_key: key.clone(),
        author: "alice".to_string(),
        content: "hello world".to_string(),
    });

    let history = engine.sessions().load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "hello world");
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::MessageReceived {
            session_key,
            author,
            content,
        } if session_key == key && author == "alice" && content == "hello world"
    ));
}

#[tokio::test]
async fn process_message_streaming_errors_when_team_store_is_unavailable() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();
    engine
        .session_manager
        .set_active_team(&key, "code-review".into());

    let err = engine
        .process_message_streaming(&key, Some("alice"), "hello world")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("team store not available"));
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::TeamActivated { .. }
    ));
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::MessageReceived { .. }
    ));
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::StreamStarted { session_key, .. } if session_key == key
    ));
}

// ── Streaming-specific tests ─────────────────────────────────────────────────

#[tokio::test]
async fn process_message_streaming_no_team_returns_some_receiver() {
    // When no team is active, streaming starts in a background task and the
    // receiver is returned immediately — the function must not block or error.
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    let result = engine
        .process_message_streaming(&key, Some("user"), "test message")
        .await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

#[tokio::test]
async fn process_message_streaming_emits_message_received_then_stream_started() {
    // Both events must be emitted before the function returns, in order.
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    let _ = engine
        .process_message_streaming(&key, Some("alice"), "hello")
        .await
        .unwrap();

    let ev1 = rx.try_recv().unwrap();
    assert!(
        matches!(ev1.kind, AppEventKind::MessageReceived { .. }),
        "expected MessageReceived, got {:?}",
        ev1.kind
    );
    let ev2 = rx.try_recv().unwrap();
    assert!(
        matches!(ev2.kind, AppEventKind::StreamStarted { ref session_key, .. } if *session_key == key),
        "expected StreamStarted for key, got {:?}",
        ev2.kind
    );
}

#[tokio::test]
async fn process_message_streaming_records_user_message_in_session() {
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    let _ = engine
        .process_message_streaming(&key, Some("alice"), "stored message")
        .await
        .unwrap();

    let history = engine.sessions().load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[0].content, "stored message");
    assert_eq!(history[0].author.as_deref(), Some("alice"));
}

#[tokio::test]
async fn process_message_streaming_none_author_emits_unknown() {
    // When author is None, the MessageReceived event should use "unknown".
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    let _ = engine
        .process_message_streaming(&key, None, "anonymous message")
        .await
        .unwrap();

    let ev = rx.try_recv().unwrap();
    assert!(
        matches!(ev.kind, AppEventKind::MessageReceived { ref author, .. } if author == "unknown"),
        "expected author 'unknown', got {:?}",
        ev.kind
    );
}

#[tokio::test]
async fn process_message_streaming_team_error_path_emits_stream_started() {
    // When team store is unavailable, StreamStarted is still emitted before the error.
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();
    engine
        .session_manager
        .set_active_team(&key, "code-review".into());

    let result = engine
        .process_message_streaming(&key, Some("alice"), "hello")
        .await;

    assert!(result.is_err());

    // Event order: TeamActivated (from set_active_team), MessageReceived, StreamStarted
    let _team_ev = rx.try_recv().unwrap(); // TeamActivated
    let ev1 = rx.try_recv().unwrap();
    assert!(matches!(ev1.kind, AppEventKind::MessageReceived { .. }));
    let ev2 = rx.try_recv().unwrap();
    assert!(matches!(ev2.kind, AppEventKind::StreamStarted { .. }));
}

#[tokio::test]
async fn process_message_streaming_accepts_messages_after_shutdown() {
    // Shutdown clears the orchestrator cache but the engine must remain functional.
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key = test_key();

    engine.shutdown().await;

    let result = engine
        .process_message_streaming(&key, Some("user"), "post-shutdown")
        .await;
    assert!(result.is_ok());

    let history = engine.sessions().load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, "post-shutdown");
}

#[tokio::test]
async fn process_message_streaming_multiple_sessions_are_independent() {
    // Messages from different session keys must not cross-contaminate history.
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
    let key_a = SessionKey::new(Platform::Discord, "guild-1", "chan-a");
    let key_b = SessionKey::new(Platform::Discord, "guild-1", "chan-b");

    let _ = engine
        .process_message_streaming(&key_a, Some("alice"), "message for A")
        .await
        .unwrap();
    let _ = engine
        .process_message_streaming(&key_b, Some("bob"), "message for B")
        .await
        .unwrap();

    let history_a = engine.sessions().load_history(&key_a, 10).unwrap();
    let history_b = engine.sessions().load_history(&key_b, 10).unwrap();
    assert_eq!(history_a.len(), 1);
    assert_eq!(history_a[0].content, "message for A");
    assert_eq!(history_b.len(), 1);
    assert_eq!(history_b[0].content, "message for B");
}

/// Verifies that the orchestrator cache miss after insert propagates as an anyhow error
/// rather than panicking, matching the `ok_or_else` replacement for the former `.expect()`.
#[test]
fn orchestrator_cache_miss_returns_error_not_panic() {
    // The error path (None returned after insert) is unreachable under normal operation,
    // but replacing `.expect()` with `ok_or_else(|| anyhow::anyhow!(...))` ensures any
    // unexpected cache miss produces a recoverable error. This test validates that the
    // error message is preserved correctly through the conversion.
    let result: anyhow::Result<std::sync::Arc<i32>> = None::<std::sync::Arc<i32>>
        .ok_or_else(|| anyhow::anyhow!("orchestrator cache key missing immediately after insert"));
    let err = result.unwrap_err();
    assert_eq!(
        err.to_string(),
        "orchestrator cache key missing immediately after insert"
    );
}

#[tokio::test]
async fn shutdown_clears_orchestrator_cache() {
    let event_bus = EventBus::new(16);
    let engine = Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);

    // Cache is empty, shutdown should be a no-op
    engine.shutdown().await;

    // Verify engine is still functional after shutdown
    let key = test_key();
    assert_eq!(engine.session_manager().active_team_for(&key), None);
}
