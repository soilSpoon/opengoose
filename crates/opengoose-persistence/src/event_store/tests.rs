use std::sync::Arc;
use std::time::Duration as StdDuration;

use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};
use opengoose_types::{EventBus, Platform, SessionKey};
use tokio::time::{sleep, timeout};

use super::queries::{cleanup_expired_events, load_event_history};
use super::*;
use crate::db::Database;
use crate::test_helpers::test_db;

fn set_event_timestamp(db: &Arc<Database>, event_id: i32, timestamp: &str) {
    db.with(|conn| {
        diesel::sql_query("UPDATE event_history SET timestamp = ?1 WHERE id = ?2")
            .bind::<Text, _>(timestamp)
            .bind::<Integer, _>(event_id)
            .execute(conn)?;
        Ok(())
    })
    .expect("timestamp update should succeed");
}

fn load_history_direct(db: &Arc<Database>, query: &EventHistoryQuery) -> Vec<EventHistoryEntry> {
    db.with(|conn| load_event_history(conn, query))
        .expect("history should load")
}

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
fn load_event_history_orders_same_timestamps_by_id_and_supports_pagination() {
    let db = test_db();
    let store = EventStore::new(db.clone());

    let first = store
        .record(&AppEventKind::GooseReady)
        .expect("first event should persist");
    let second = store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Slack,
        })
        .expect("second event should persist");
    let third = store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Discord,
        })
        .expect("third event should persist");

    set_event_timestamp(&db, first.id, "2026-03-10 12:00:00");
    set_event_timestamp(&db, second.id, "2026-03-10 12:00:00");
    set_event_timestamp(&db, third.id, "2026-03-10 12:05:00");

    let all_entries = load_history_direct(
        &db,
        &EventHistoryQuery {
            limit: 10,
            ..EventHistoryQuery::default()
        },
    );
    let paged_entries = load_history_direct(
        &db,
        &EventHistoryQuery {
            limit: 1,
            offset: 1,
            ..EventHistoryQuery::default()
        },
    );

    assert_eq!(
        all_entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![third.id, second.id, first.id]
    );
    assert_eq!(
        paged_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        vec![second.id]
    );
}

#[test]
fn load_event_history_filters_by_session_key_and_since_boundary() {
    let db = test_db();
    let store = EventStore::new(db.clone());

    let older_alpha = store
        .record(&AppEventKind::MessageReceived {
            session_key: SessionKey::new(Platform::Discord, "ops", "alpha"),
            author: "alice".into(),
            content: "older".into(),
        })
        .expect("older alpha event should persist");
    let boundary_alpha = store
        .record(&AppEventKind::MessageReceived {
            session_key: SessionKey::new(Platform::Discord, "ops", "alpha"),
            author: "bob".into(),
            content: "boundary".into(),
        })
        .expect("boundary alpha event should persist");
    let newer_beta = store
        .record(&AppEventKind::MessageReceived {
            session_key: SessionKey::new(Platform::Discord, "ops", "beta"),
            author: "carol".into(),
            content: "newer".into(),
        })
        .expect("newer beta event should persist");

    set_event_timestamp(&db, older_alpha.id, "2026-03-10 12:00:00");
    set_event_timestamp(&db, boundary_alpha.id, "2026-03-10 12:05:00");
    set_event_timestamp(&db, newer_beta.id, "2026-03-10 12:10:00");

    let filtered = load_history_direct(
        &db,
        &EventHistoryQuery {
            limit: 10,
            session_key: Some("discord:ns:ops:alpha".into()),
            since: Some("2026-03-10 12:05:00".into()),
            ..EventHistoryQuery::default()
        },
    );
    let empty = load_history_direct(
        &db,
        &EventHistoryQuery {
            session_key: Some("discord:ns:ops:missing".into()),
            ..EventHistoryQuery::default()
        },
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, boundary_alpha.id);
    assert_eq!(
        filtered[0].session_key.as_deref(),
        Some("discord:ns:ops:alpha")
    );
    assert!(empty.is_empty());
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
fn cleanup_expired_events_only_removes_rows_older_than_cutoff() {
    let db = test_db();
    let store = EventStore::new(db.clone());

    let stale = store
        .record(&AppEventKind::GooseReady)
        .expect("stale event should persist");
    let fresh = store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Slack,
        })
        .expect("fresh event should persist");

    db.with(|conn| {
        diesel::sql_query(
            "UPDATE event_history SET timestamp = datetime('now', '-3 days') WHERE id = ?1",
        )
        .bind::<Integer, _>(stale.id)
        .execute(conn)?;
        diesel::sql_query(
            "UPDATE event_history SET timestamp = datetime('now', '-6 hours') WHERE id = ?1",
        )
        .bind::<Integer, _>(fresh.id)
        .execute(conn)?;

        let deleted = cleanup_expired_events(conn, 1)?;
        let remaining = load_event_history(conn, &EventHistoryQuery::default())?;

        assert_eq!(deleted, 1);
        assert_eq!(
            remaining.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![fresh.id]
        );

        Ok(())
    })
    .expect("cleanup query should succeed");
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

#[test]
fn normalize_since_filter_supports_sqlite_and_date_inputs() {
    let sqlite =
        normalize_since_filter("2026-03-10 12:00:00").expect("sqlite timestamp should parse");
    let date = normalize_since_filter("2026-03-10").expect("date should parse");

    assert_eq!(sqlite, "2026-03-10 12:00:00");
    assert_eq!(date, "2026-03-10 00:00:00");
}

#[test]
fn normalize_since_filter_rejects_empty_and_invalid_relative_values() {
    let empty = normalize_since_filter("   ").expect_err("empty filters should fail");
    let missing_number = normalize_since_filter("h").expect_err("missing number should fail");
    let fractional = normalize_since_filter("1.5h").expect_err("fractional hours should fail");

    assert_eq!(empty, "`since` must not be empty");
    assert!(missing_number.contains("invalid relative `since` value `h`"));
    assert!(fractional.contains("invalid relative `since` value `1.5h`"));
}
