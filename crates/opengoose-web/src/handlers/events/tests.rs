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

// ── snapshot.rs: LiveEventType coverage ─────────────────────────────

#[test]
fn live_event_type_as_str_all_variants() {
    assert_eq!(LiveEventType::Dashboard.as_str(), "dashboard");
    assert_eq!(LiveEventType::Session.as_str(), "session");
    assert_eq!(LiveEventType::Run.as_str(), "run");
    assert_eq!(LiveEventType::Queue.as_str(), "queue");
    assert_eq!(LiveEventType::Channel.as_str(), "channel");
    assert_eq!(LiveEventType::Error.as_str(), "error");
}

#[test]
fn event_filter_parses_all_valid_types() {
    // Tests LiveEventType::parse indirectly through EventFilter
    for name in ["dashboard", "session", "run", "queue", "channel", "error"] {
        assert!(
            EventFilter::parse(Some(name)).is_ok(),
            "filter should accept '{name}'"
        );
    }
}

#[test]
fn event_filter_parse_case_insensitive() {
    assert!(EventFilter::parse(Some("DASHBOARD")).is_ok());
    assert!(EventFilter::parse(Some("Session")).is_ok());
    assert!(EventFilter::parse(Some("RUN")).is_ok());
}

#[test]
fn event_filter_parse_invalid_type_returns_error() {
    assert!(EventFilter::parse(Some("bogus")).is_err());
}

// ── snapshot.rs: EventFilter coverage ───────────────────────────────

#[test]
fn event_filter_none_allows_all_via_serialize() {
    let filter = EventFilter::parse(None).unwrap();
    // Default filter should let through all event types
    assert!(serialize_app_event(&AppEventKind::DashboardUpdated, &filter).is_some());
    assert!(
        serialize_app_event(
            &AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id("x:y:z"),
            },
            &filter,
        )
        .is_some()
    );
    assert!(
        serialize_app_event(
            &AppEventKind::TeamRunStarted {
                team: "t".into(),
                workflow: "c".into(),
                input: "i".into(),
            },
            &filter,
        )
        .is_some()
    );
    assert!(
        serialize_app_event(&AppEventKind::QueueUpdated { team_run_id: None }, &filter,).is_some()
    );
    assert!(serialize_app_event(&AppEventKind::GooseReady, &filter).is_some());
    assert!(
        serialize_app_event(
            &AppEventKind::Error {
                context: "c".into(),
                message: "m".into(),
            },
            &filter,
        )
        .is_some()
    );
}

#[test]
fn event_filter_empty_string_allows_all() {
    let filter = EventFilter::parse(Some("")).unwrap();
    assert!(serialize_app_event(&AppEventKind::DashboardUpdated, &filter).is_some());
}

#[test]
fn event_filter_whitespace_only_allows_all() {
    let filter = EventFilter::parse(Some("   ")).unwrap();
    assert!(
        serialize_app_event(
            &AppEventKind::TeamRunStarted {
                team: "t".into(),
                workflow: "c".into(),
                input: "i".into(),
            },
            &filter,
        )
        .is_some()
    );
}

#[test]
fn event_filter_single_type_allows_matching_blocks_others() {
    let filter = EventFilter::parse(Some("run")).unwrap();
    // Run events should pass
    assert!(
        serialize_app_event(
            &AppEventKind::TeamRunStarted {
                team: "t".into(),
                workflow: "c".into(),
                input: "i".into(),
            },
            &filter,
        )
        .is_some()
    );
    // Session events should be blocked
    assert!(
        serialize_app_event(
            &AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id("x:y:z"),
            },
            &filter,
        )
        .is_none()
    );
    // Dashboard should be blocked
    assert!(serialize_app_event(&AppEventKind::DashboardUpdated, &filter).is_none());
}

#[test]
fn event_filter_multiple_types() {
    let filter = EventFilter::parse(Some("run,session,error")).unwrap();
    assert!(
        serialize_app_event(
            &AppEventKind::TeamRunCompleted { team: "t".into() },
            &filter,
        )
        .is_some()
    );
    assert!(
        serialize_app_event(
            &AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id("x:y:z"),
            },
            &filter,
        )
        .is_some()
    );
    assert!(
        serialize_app_event(
            &AppEventKind::Error {
                context: "c".into(),
                message: "m".into(),
            },
            &filter,
        )
        .is_some()
    );
    // Dashboard should be blocked
    assert!(serialize_app_event(&AppEventKind::DashboardUpdated, &filter).is_none());
    // Queue should be blocked
    assert!(
        serialize_app_event(&AppEventKind::QueueUpdated { team_run_id: None }, &filter,).is_none()
    );
}

#[test]
fn event_filter_invalid_type_errors() {
    let result = EventFilter::parse(Some("run,bogus"));
    assert!(result.is_err());
}

// ── snapshot.rs: serialize_app_event coverage ───────────────────────

#[test]
fn dashboard_event_serializes() {
    let result = serialize_app_event(&AppEventKind::DashboardUpdated, &EventFilter::default());
    let ev = result.expect("should serialize");
    assert_eq!(ev.event, LiveEventType::Dashboard);
    assert!(ev.data.contains("\"type\":\"dashboard\""));
}

#[test]
fn run_updated_event_includes_status_and_run_id() {
    let result = serialize_app_event(
        &AppEventKind::RunUpdated {
            team_run_id: "run-42".into(),
            status: "completed".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Run);
    assert!(ev.data.contains("\"teamRunId\":\"run-42\""));
    assert!(ev.data.contains("\"status\":\"completed\""));
}

#[test]
fn team_run_started_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamRunStarted {
            team: "alpha".into(),
            workflow: "chain".into(),
            input: "hello".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Run);
    assert!(ev.data.contains("\"status\":\"started\""));
}

#[test]
fn team_step_started_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamStepStarted {
            team: "t".into(),
            agent: "a".into(),
            step: 0,
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Run);
    assert!(ev.data.contains("\"status\":\"step_started\""));
}

#[test]
fn team_step_completed_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamStepCompleted {
            team: "t".into(),
            agent: "a".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert!(ev.data.contains("\"status\":\"step_completed\""));
}

#[test]
fn team_step_failed_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamStepFailed {
            team: "t".into(),
            agent: "a".into(),
            reason: "oops".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert!(ev.data.contains("\"status\":\"step_failed\""));
}

#[test]
fn team_run_completed_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamRunCompleted { team: "t".into() },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert!(ev.data.contains("\"status\":\"completed\""));
}

#[test]
fn team_run_failed_event() {
    let result = serialize_app_event(
        &AppEventKind::TeamRunFailed {
            team: "t".into(),
            reason: "crash".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert!(ev.data.contains("\"status\":\"failed\""));
}

#[test]
fn queue_updated_event_with_run_id() {
    let result = serialize_app_event(
        &AppEventKind::QueueUpdated {
            team_run_id: Some("run-99".into()),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Queue);
    assert!(ev.data.contains("\"teamRunId\":\"run-99\""));
}

#[test]
fn queue_updated_event_without_run_id() {
    let result = serialize_app_event(
        &AppEventKind::QueueUpdated { team_run_id: None },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Queue);
    assert!(!ev.data.contains("teamRunId"));
}

#[test]
fn channel_ready_event() {
    let result = serialize_app_event(
        &AppEventKind::ChannelReady {
            platform: Platform::Slack,
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
}

#[test]
fn channel_disconnected_event() {
    let result = serialize_app_event(
        &AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: "timeout".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
}

#[test]
fn goose_ready_event() {
    let result = serialize_app_event(&AppEventKind::GooseReady, &EventFilter::default());
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
}

#[test]
fn error_event() {
    let result = serialize_app_event(
        &AppEventKind::Error {
            context: "test".into(),
            message: "bad".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Error);
}

#[test]
fn tracing_event() {
    let result = serialize_app_event(
        &AppEventKind::TracingEvent {
            level: "WARN".into(),
            message: "something".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Error);
}

#[test]
fn alert_fired_event() {
    let result = serialize_app_event(
        &AppEventKind::AlertFired {
            rule_name: "high-queue".into(),
            metric: "queue_backlog".into(),
            value: 150.0,
            platform: "slack".into(),
            channel_id: "C123".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
    assert!(ev.data.contains("\"status\":\"alert_fired\""));
}

#[test]
fn shutdown_started_event() {
    let result = serialize_app_event(
        &AppEventKind::ShutdownStarted {
            timeout_secs: 30,
            active_streams: 2,
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
}

#[test]
fn pairing_code_generated_event() {
    let result = serialize_app_event(
        &AppEventKind::PairingCodeGenerated {
            code: "ABC123".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Channel);
}

#[test]
fn stream_started_maps_to_session() {
    let result = serialize_app_event(
        &AppEventKind::StreamStarted {
            session_key: SessionKey::from_stable_id("discord:ns:ch:room"),
            stream_id: "s1".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Session);
    assert!(ev.data.contains("\"sessionKey\""));
}

#[test]
fn model_changed_maps_to_session() {
    let result = serialize_app_event(
        &AppEventKind::ModelChanged {
            session_key: SessionKey::from_stable_id("matrix:ns:ch"),
            model: "gpt-4".into(),
            mode: "fast".into(),
        },
        &EventFilter::default(),
    );
    let ev = result.unwrap();
    assert_eq!(ev.event, LiveEventType::Session);
}

#[test]
fn filter_blocks_non_matching_then_allows_matching() {
    let filter = EventFilter::parse(Some("error")).unwrap();

    let blocked = serialize_app_event(
        &AppEventKind::SessionUpdated {
            session_key: SessionKey::from_stable_id("x:y:z"),
        },
        &filter,
    );
    assert!(blocked.is_none());

    let allowed = serialize_app_event(
        &AppEventKind::Error {
            context: "t".into(),
            message: "m".into(),
        },
        &filter,
    );
    assert!(allowed.is_some());
}
