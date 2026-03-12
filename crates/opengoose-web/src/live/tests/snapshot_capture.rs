use opengoose_persistence::SessionStore;
use opengoose_types::{Platform, SessionKey};

use super::super::snapshot::capture_live_snapshot;
use super::make_db;

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
