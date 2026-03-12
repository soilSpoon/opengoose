use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use futures_util::StreamExt;
use opengoose_persistence::EventStore;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio::time::timeout;

use super::poll::{EventHistoryQueryParams, list_event_history};
use super::snapshot::{EventFilter, LiveEventType, serialize_app_event};
use super::stream::build_event_stream;
use crate::handlers::test_support::make_state;

#[test]
fn session_event_serializes_expected_payload() {
    let serialized = serialize_app_event(
        &AppEventKind::SessionUpdated {
            session_key: SessionKey::from_stable_id("discord:ns:ops:bridge"),
        },
        &EventFilter::default(),
    )
    .expect("session event should serialize");

    assert_eq!(serialized.event, LiveEventType::Session);
    assert_eq!(
        serialized.data,
        r#"{"type":"session","sessionKey":"discord:ns:ops:bridge"}"#
    );
}

#[test]
fn filter_excludes_non_matching_event_types() {
    let filter = EventFilter::parse(Some("run")).expect("filter should parse");

    let serialized = serialize_app_event(
        &AppEventKind::ChannelReady {
            platform: Platform::Slack,
        },
        &filter,
    );

    assert!(serialized.is_none());
}

#[tokio::test]
async fn event_stream_finishes_cleanly_when_bus_closes() {
    let bus = EventBus::new(8);
    let stream = build_event_stream(bus.subscribe(), EventFilter::default());
    tokio::pin!(stream);

    drop(bus);

    let next = timeout(Duration::from_millis(100), stream.next())
        .await
        .expect("stream should stop promptly");

    assert!(next.is_none());
}

#[tokio::test]
async fn list_event_history_returns_persisted_entries() {
    let state = make_state();
    let store = EventStore::new(state.db.clone());
    store
        .record(&AppEventKind::ChannelReady {
            platform: Platform::Discord,
        })
        .expect("event should persist");

    let Json(page) = list_event_history(
        State(state),
        Query(EventHistoryQueryParams {
            limit: 10,
            offset: 0,
            gateway: Some("discord".into()),
            kind: Some("channel_ready".into()),
            session_key: None,
            since: None,
        }),
    )
    .await
    .expect("history query should succeed");

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].event_kind, "channel_ready");
    assert_eq!(page.items[0].source_gateway.as_deref(), Some("discord"));
    assert!(!page.has_more);
}

#[tokio::test]
async fn list_event_history_rejects_invalid_limit() {
    let state = make_state();
    let result = list_event_history(
        State(state),
        Query(EventHistoryQueryParams {
            limit: 0,
            offset: 0,
            gateway: None,
            kind: None,
            session_key: None,
            since: None,
        }),
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn list_event_history_rejects_invalid_since_filter() {
    let state = make_state();
    let result = list_event_history(
        State(state),
        Query(EventHistoryQueryParams {
            limit: 10,
            offset: 0,
            gateway: None,
            kind: None,
            session_key: None,
            since: Some("definitely-not-a-time".into()),
        }),
    )
    .await;

    assert!(result.is_err());
}
