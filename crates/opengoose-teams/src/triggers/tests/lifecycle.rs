use std::{path::Path, sync::Arc, time::Duration};

use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio_util::sync::CancellationToken;

use super::super::*;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

fn trigger_store(db: &Arc<Database>) -> TriggerStore {
    TriggerStore::new(db.clone())
}

fn file_watch_condition(path: &Path) -> String {
    serde_json::to_string(&FileWatchCondition {
        pattern: Some(format!("{}/*.tmp", path.display())),
    })
    .unwrap()
}

fn fire_count(db: &Arc<Database>, name: &str) -> i32 {
    trigger_store(db)
        .get_by_name(name)
        .unwrap()
        .unwrap()
        .fire_count
}

fn last_fired_at(db: &Arc<Database>, name: &str) -> Option<String> {
    trigger_store(db)
        .get_by_name(name)
        .unwrap()
        .unwrap()
        .last_fired_at
}

async fn wait_for_fire_count(db: &Arc<Database>, name: &str, expected: i32) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if fire_count(db, name) >= expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("trigger did not reach expected fire count");
}

#[tokio::test]
async fn test_file_watch_trigger_watcher_cancels_cleanly() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let cancel = CancellationToken::new();

    let handle = spawn_file_watch_trigger_watcher(db.clone(), event_bus, cancel.clone());

    // Give the task time to start up, then cancel it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    // Should finish promptly after cancellation.
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_file_watch_trigger_fires_on_matching_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = test_db();
    let event_bus = EventBus::new(64);
    let condition = file_watch_condition(dir.path());
    let tmp_file = dir.path().join("test.tmp");
    std::fs::write(&tmp_file, b"hello").unwrap();

    trigger_store(&db)
        .create("watch-test", "file_watch", &condition, "no-such-team", "")
        .unwrap();

    fire_file_watch_triggers(&db, &event_bus, &tmp_file.to_string_lossy())
        .await
        .unwrap();
    wait_for_fire_count(&db, "watch-test", 1).await;
    assert!(last_fired_at(&db, "watch-test").is_some());
}

#[tokio::test]
async fn test_file_watch_trigger_ignores_non_matching_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = test_db();
    let event_bus = EventBus::new(64);
    let condition = file_watch_condition(dir.path());
    trigger_store(&db)
        .create("watch-test", "file_watch", &condition, "no-such-team", "")
        .unwrap();

    let ignored = dir.path().join("ignored.txt");
    std::fs::write(&ignored, b"hello").unwrap();
    fire_file_watch_triggers(&db, &event_bus, &ignored.to_string_lossy())
        .await
        .unwrap();

    assert_eq!(fire_count(&db, "watch-test"), 0);
    assert!(last_fired_at(&db, "watch-test").is_none());
}

#[tokio::test]
async fn test_trigger_watcher_cancels_cleanly() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let message_bus = crate::message_bus::MessageBus::new(64);
    let cancel = CancellationToken::new();

    let handle = spawn_trigger_watcher(db, event_bus, message_bus, cancel.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_trigger_watcher_marks_matching_message_trigger_fired() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let message_bus = crate::message_bus::MessageBus::new(64);
    let cancel = CancellationToken::new();

    trigger_store(&db)
        .create(
            "message-trigger",
            "message_received",
            r#"{"from_agent":"agent-a","channel":"alerts","payload_contains":"critical"}"#,
            "no-such-team",
            "",
        )
        .unwrap();

    let handle = spawn_trigger_watcher(db.clone(), event_bus, message_bus.clone(), cancel.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;
    message_bus.publish("agent-a", "alerts", "critical failure");

    wait_for_fire_count(&db, "message-trigger", 1).await;
    assert!(last_fired_at(&db, "message-trigger").is_some());

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_trigger_watcher_ignores_non_matching_message() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let message_bus = crate::message_bus::MessageBus::new(64);
    let cancel = CancellationToken::new();

    trigger_store(&db)
        .create(
            "message-trigger",
            "message_received",
            r#"{"from_agent":"agent-a","channel":"alerts","payload_contains":"critical"}"#,
            "no-such-team",
            "",
        )
        .unwrap();

    let handle = spawn_trigger_watcher(db.clone(), event_bus, message_bus.clone(), cancel.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;
    message_bus.publish("agent-b", "alerts", "critical failure");
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(fire_count(&db, "message-trigger"), 0);
    assert!(last_fired_at(&db, "message-trigger").is_none());

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_event_bus_trigger_watcher_cancels_cleanly() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let cancel = CancellationToken::new();

    let handle = spawn_event_bus_trigger_watcher(db, event_bus, cancel.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}

#[tokio::test]
async fn test_event_bus_trigger_watcher_marks_matching_events_fired() {
    let db = test_db();
    let event_bus = EventBus::new(64);
    let cancel = CancellationToken::new();
    let session_key = SessionKey::direct(Platform::Discord, "channel-1");

    let store = trigger_store(&db);
    store
        .create(
            "on-message",
            "on_message",
            r#"{"from_author":"alice","content_contains":"deploy"}"#,
            "no-such-team",
            "",
        )
        .unwrap();
    store
        .create(
            "on-start",
            "on_session_start",
            r#"{"platform":"discord"}"#,
            "no-such-team",
            "",
        )
        .unwrap();
    store
        .create(
            "on-end",
            "on_session_end",
            r#"{"platform":"discord"}"#,
            "no-such-team",
            "",
        )
        .unwrap();
    store
        .create(
            "on-schedule",
            "on_schedule",
            r#"{"team":"release-team"}"#,
            "no-such-team",
            "",
        )
        .unwrap();

    let handle = spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;
    event_bus.emit(AppEventKind::MessageReceived {
        session_key: session_key.clone(),
        author: "alice".into(),
        content: "deploy now".into(),
    });
    event_bus.emit(AppEventKind::ChannelReady {
        platform: Platform::Discord,
    });
    event_bus.emit(AppEventKind::SessionDisconnected {
        session_key,
        reason: "test".into(),
    });
    event_bus.emit(AppEventKind::TeamRunCompleted {
        team: "release-team".into(),
    });

    wait_for_fire_count(&db, "on-message", 1).await;
    wait_for_fire_count(&db, "on-start", 1).await;
    wait_for_fire_count(&db, "on-end", 1).await;
    wait_for_fire_count(&db, "on-schedule", 1).await;

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("watcher did not stop within timeout")
        .expect("watcher task panicked");
}
