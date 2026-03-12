use std::time::Duration as StdDuration;

use opengoose_types::{EventBus, Platform, SessionKey};
use tokio::time::{sleep, timeout};

use super::*;
use crate::test_helpers::test_db;

#[test]
fn record_and_list_roundtrip() {
    let store = EventStore::new(test_db());

    store
        .record(&AppEventKind::MessageReceived {
            session_key: SessionKey::new(Platform::Discord, "ops", "bridge"),
            author: "alice".into(),
            content: "hello".into(),
        })
        .expect("event should be recorded");

    let entries = store
        .list(&EventHistoryQuery::default())
        .expect("history should load");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_kind, "message_received");
    assert_eq!(entries[0].source_gateway.as_deref(), Some("discord"));
    assert_eq!(
        entries[0].session_key.as_deref(),
        Some("discord:ns:ops:bridge")
    );
    assert_eq!(entries[0].payload["type"], "message_received");
}

#[test]
fn list_filters_by_gateway_and_kind() {
    let store = EventStore::new(test_db());

    store
        .record(&AppEventKind::GooseReady)
        .expect("goose event should persist");
    store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Slack,
        })
        .expect("channel event should persist");

    let entries = store
        .list(&EventHistoryQuery {
            source_gateway: Some("slack".into()),
            event_kind: Some("channel_ready".into()),
            ..EventHistoryQuery::default()
        })
        .expect("filtered history should load");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_kind, "channel_ready");
    assert_eq!(entries[0].source_gateway.as_deref(), Some("slack"));
}

#[test]
fn cleanup_expired_deletes_old_events() {
    let db = test_db();
    let store = EventStore::new(db.clone());

    let event = store
        .record(&AppEventKind::GooseReady)
        .expect("event should persist");

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE event_history SET timestamp = datetime('now', '-40 days') WHERE id = ?1",
        )
        .bind::<diesel::sql_types::Integer, _>(event.id)
        .execute(conn)?;
        Ok(())
    })
    .expect("timestamp update should succeed");

    let deleted = store.cleanup_expired(30).expect("cleanup should succeed");

    assert_eq!(deleted, 1);
    assert!(
        store
            .list(&EventHistoryQuery::default())
            .expect("history should load")
            .is_empty()
    );
}

#[test]
fn replay_reemits_persisted_events() {
    let store = EventStore::new(test_db());
    let replay_bus = EventBus::new(8);
    let mut rx = replay_bus.subscribe();

    store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Discord,
        })
        .expect("event should persist");

    let replayed = store
        .replay(&EventHistoryQuery::default(), &replay_bus)
        .expect("replay should succeed");
    let replayed_event = rx.try_recv().expect("event should be replayed");

    assert_eq!(replayed, 1);
    assert!(matches!(
        replayed_event.kind,
        AppEventKind::ChannelReady {
            platform: Platform::Discord
        }
    ));
}

#[tokio::test]
async fn recorder_persists_events_from_reliable_tap() {
    let db = test_db();
    let store = EventStore::new(db.clone());
    let bus = EventBus::new(1);

    let recorder = spawn_event_history_recorder(db, bus.clone());
    bus.emit(AppEventKind::GooseReady);

    timeout(StdDuration::from_secs(1), async {
        loop {
            if let Ok(entries) = store.list(&EventHistoryQuery::default())
                && !entries.is_empty()
            {
                break;
            }
            sleep(StdDuration::from_millis(10)).await;
        }
    })
    .await
    .expect("event should be recorded");

    assert!(recorder.flush(StdDuration::from_secs(1)).await);
    assert!(recorder.shutdown(StdDuration::from_secs(1)).await);

    let entries = store
        .list(&EventHistoryQuery::default())
        .expect("history should load");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].event_kind, "goose_ready");
}

#[test]
fn normalize_since_filter_supports_relative_and_absolute_values() {
    let relative = normalize_since_filter("24h").expect("relative filter should parse");
    let absolute =
        normalize_since_filter("2026-03-10T12:00:00Z").expect("rfc3339 filter should parse");

    assert_eq!(relative.len(), 19);
    assert_eq!(absolute, "2026-03-10 12:00:00");
}
