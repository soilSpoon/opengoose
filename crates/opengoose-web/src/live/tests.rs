use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_types::{AppEvent, AppEventKind, EventBus};

use super::changes::emit_live_snapshot_changes;
use super::snapshot::{LiveSnapshot, capture_live_snapshot};

fn make_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("in-memory db"))
}

// -- capture_live_snapshot --------------------------------------------------

#[test]
fn capture_live_snapshot_empty_db_returns_defaults() {
    let db = make_db();
    let snap = capture_live_snapshot(db).expect("snapshot should succeed");
    assert!(snap.sessions.is_empty());
    assert!(snap.runs.is_empty());
    assert_eq!(snap.queue.pending, 0);
    assert_eq!(snap.queue.processing, 0);
    assert_eq!(snap.queue.completed, 0);
    assert_eq!(snap.queue.failed, 0);
    assert_eq!(snap.queue.dead, 0);
    assert!(snap.queue.last_message_id.is_none());
    assert!(snap.queue.last_team_run_id.is_none());
}

#[test]
fn capture_live_snapshot_with_session_populates_sessions() {
    use opengoose_persistence::SessionStore;
    use opengoose_types::{Platform, SessionKey};

    let db = make_db();
    let session_store = SessionStore::new(db.clone());
    let key = SessionKey::new(Platform::Discord, "guild-1", "channel-1");
    session_store
        .append_user_message(&key, "hello", Some("user"))
        .expect("append should succeed");

    let snap = capture_live_snapshot(db).expect("snapshot should succeed");
    assert_eq!(snap.sessions.len(), 1);
    assert!(snap.sessions.contains_key(&key.to_stable_id()));
}

#[test]
fn capture_live_snapshot_is_ok_result() {
    let db = make_db();
    let result = capture_live_snapshot(db);
    assert!(result.is_ok());
}

// -- emit_live_snapshot_changes ---------------------------------------------

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<AppEventKind> {
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev.kind);
    }
    events
}

#[test]
fn identical_snapshots_emit_no_events() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let snap = LiveSnapshot::default();
    emit_live_snapshot_changes(&snap, &snap, &event_bus);

    let events = drain_events(&mut rx);
    assert!(events.is_empty(), "expected no events, got {events:?}");
}

#[test]
fn new_session_emits_session_updated_and_dashboard_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let previous = LiveSnapshot::default();
    let mut current = LiveSnapshot::default();
    current.sessions.insert(
        "discord:guild-1:chan-1".to_string(),
        "2026-01-01T00:00:00".to_string(),
    );

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::SessionUpdated { .. })),
        "expected SessionUpdated",
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated",
    );
}

#[test]
fn updated_session_timestamp_emits_session_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut previous = LiveSnapshot::default();
    previous.sessions.insert(
        "discord:guild-1:chan-1".to_string(),
        "2026-01-01T00:00:00".to_string(),
    );

    let mut current = LiveSnapshot::default();
    current.sessions.insert(
        "discord:guild-1:chan-1".to_string(),
        "2026-01-01T00:01:00".to_string(),
    );

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::SessionUpdated { .. })),
        "expected SessionUpdated on timestamp change",
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated",
    );
}

#[test]
fn session_removed_emits_dashboard_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut previous = LiveSnapshot::default();
    previous.sessions.insert(
        "discord:guild-1:chan-1".to_string(),
        "2026-01-01T00:00:00".to_string(),
    );
    let current = LiveSnapshot::default();

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated when session count changes",
    );
}

#[test]
fn new_run_emits_run_updated_and_dashboard_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let previous = LiveSnapshot::default();
    let mut current = LiveSnapshot::default();
    current.runs.insert(
        "run-abc".to_string(),
        ("2026-01-01T00:00:00".to_string(), "running".to_string()),
    );

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events.iter().any(
            |e| matches!(e, AppEventKind::RunUpdated { team_run_id, .. } if team_run_id == "run-abc")
        ),
        "expected RunUpdated for run-abc",
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated",
    );
}

#[test]
fn run_status_change_emits_run_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut previous = LiveSnapshot::default();
    previous.runs.insert(
        "run-abc".to_string(),
        ("2026-01-01T00:00:00".to_string(), "running".to_string()),
    );

    let mut current = LiveSnapshot::default();
    current.runs.insert(
        "run-abc".to_string(),
        ("2026-01-01T00:01:00".to_string(), "completed".to_string()),
    );

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::RunUpdated { status, .. } if status == "completed")),
        "expected RunUpdated with completed status",
    );
}

#[test]
fn run_removed_emits_dashboard_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut previous = LiveSnapshot::default();
    previous.runs.insert(
        "run-abc".to_string(),
        ("2026-01-01T00:00:00".to_string(), "running".to_string()),
    );
    let current = LiveSnapshot::default();

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated when run count changes",
    );
}

#[test]
fn queue_stats_change_emits_queue_updated_and_dashboard_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let previous = LiveSnapshot::default();
    let mut current = LiveSnapshot::default();
    current.queue.pending = 3;
    current.queue.last_team_run_id = Some("run-xyz".to_string());

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::QueueUpdated { .. })),
        "expected QueueUpdated",
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEventKind::DashboardUpdated)),
        "expected DashboardUpdated",
    );
}

#[test]
fn queue_unchanged_emits_no_queue_event() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut snap = LiveSnapshot::default();
    snap.queue.pending = 5;
    snap.queue.processing = 2;

    emit_live_snapshot_changes(&snap, &snap, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEventKind::QueueUpdated { .. })),
        "expected no QueueUpdated when queue is unchanged",
    );
}

#[test]
fn queue_updated_carries_last_team_run_id() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let previous = LiveSnapshot::default();
    let mut current = LiveSnapshot::default();
    current.queue.completed = 1;
    current.queue.last_team_run_id = Some("team-run-42".to_string());

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    let queue_event = events
        .iter()
        .find(|e| matches!(e, AppEventKind::QueueUpdated { .. }));
    assert!(queue_event.is_some());
    if let Some(AppEventKind::QueueUpdated { team_run_id }) = queue_event {
        assert_eq!(team_run_id.as_deref(), Some("team-run-42"));
    }
}

#[test]
fn multiple_session_changes_emit_multiple_session_updated_events() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut previous = LiveSnapshot::default();
    previous
        .sessions
        .insert("session-a".to_string(), "t1".to_string());
    previous
        .sessions
        .insert("session-b".to_string(), "t1".to_string());

    let mut current = LiveSnapshot::default();
    current
        .sessions
        .insert("session-a".to_string(), "t2".to_string());
    current
        .sessions
        .insert("session-b".to_string(), "t1".to_string());
    current
        .sessions
        .insert("session-c".to_string(), "t1".to_string());

    emit_live_snapshot_changes(&previous, &current, &event_bus);

    let events = drain_events(&mut rx);
    let session_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, AppEventKind::SessionUpdated { .. }))
        .collect();
    assert_eq!(
        session_events.len(),
        2,
        "expected 2 SessionUpdated events, got {}",
        session_events.len(),
    );

    let dashboard_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, AppEventKind::DashboardUpdated))
        .collect();
    assert_eq!(
        dashboard_events.len(),
        1,
        "expected exactly 1 DashboardUpdated",
    );
}

#[test]
fn unchanged_session_emits_no_session_updated() {
    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();

    let mut snap = LiveSnapshot::default();
    snap.sessions
        .insert("session-a".to_string(), "2026-01-01T00:00:00".to_string());

    emit_live_snapshot_changes(&snap, &snap, &event_bus);

    let events = drain_events(&mut rx);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEventKind::SessionUpdated { .. })),
        "expected no SessionUpdated for unchanged sessions",
    );
}
