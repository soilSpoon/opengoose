use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use diesel::Connection;
use diesel::RunQueryDsl;
use diesel::sql_query;
use diesel::sqlite::SqliteConnection;
use opengoose_persistence::{Database, SessionStore};
use opengoose_types::{Platform, SessionKey};

use super::*;

fn session_entry(session_key: SessionKey) -> SessionListEntry {
    SessionListEntry {
        session_key,
        active_team: None,
        created_at: None,
        updated_at: None,
        is_active: false,
    }
}

fn test_app_with_sessions(count: usize) -> (App, Vec<SessionKey>) {
    let mut app = App::new(AppMode::Normal, None, None);
    let mut keys = Vec::new();
    for i in 0..count {
        let sk = SessionKey::dm(Platform::Discord, format!("user-{i}"));
        app.sessions.push(session_entry(sk.clone()));
        keys.push(sk);
    }
    (app, keys)
}

fn file_backed_session_store() -> (Arc<SessionStore>, tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions.db");
    let db = Arc::new(Database::open_at(path.clone()).unwrap());
    let store = Arc::new(SessionStore::new(db));
    (store, dir, path)
}

fn execute_sql(path: &Path, sql: &str) {
    let mut conn = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
    sql_query(sql).execute(&mut conn).unwrap();
}

// ── Session navigation ─────────────────────────────────────────

#[test]
fn test_select_next_session_advances() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(0);

    app.select_next_session();
    assert_eq!(app.selected_session, Some(keys[1].clone()));
    assert_eq!(app.selected_session_index, 1);
}

#[test]
fn test_select_next_session_clamps_at_end() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(2);

    app.select_next_session();
    assert_eq!(app.selected_session, Some(keys[2].clone()));
    assert_eq!(app.selected_session_index, 2);
}

#[test]
fn test_select_next_session_empty_is_noop() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.select_next_session();
    assert!(app.selected_session.is_none());
}

#[test]
fn test_select_previous_session_decrements() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(2);

    app.select_previous_session();
    assert_eq!(app.selected_session, Some(keys[1].clone()));
    assert_eq!(app.selected_session_index, 1);
}

#[test]
fn test_select_previous_session_clamps_at_zero() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(0);

    app.select_previous_session();
    assert_eq!(app.selected_session, Some(keys[0].clone()));
    assert_eq!(app.selected_session_index, 0);
}

#[test]
fn test_select_previous_session_empty_is_noop() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.select_previous_session();
    assert!(app.selected_session.is_none());
}

#[test]
fn test_select_first_session() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(2);

    app.select_first_session();
    assert_eq!(app.selected_session, Some(keys[0].clone()));
    assert_eq!(app.selected_session_index, 0);
}

#[test]
fn test_select_first_session_empty_is_noop() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.select_first_session();
    assert!(app.selected_session.is_none());
}

#[test]
fn test_select_last_session() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(0);

    app.select_last_session();
    assert_eq!(app.selected_session, Some(keys[2].clone()));
    assert_eq!(app.selected_session_index, 2);
}

#[test]
fn test_select_last_session_empty_is_noop() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.select_last_session();
    assert!(app.selected_session.is_none());
}

#[test]
fn test_select_session_clamps_index() {
    let (mut app, keys) = test_app_with_sessions(2);
    app.select_session(100);
    assert_eq!(app.selected_session_index, 1);
    assert_eq!(app.selected_session, Some(keys[1].clone()));
}

#[test]
fn test_select_session_resets_scroll() {
    let (mut app, _keys) = test_app_with_sessions(2);
    app.messages_scroll = 10;
    app.select_session(0);
    assert_eq!(app.messages_scroll, 0);
}

// ── Focus sessions ─────────────────────────────────────────────

#[test]
fn test_focus_sessions_switches_panel() {
    let (mut app, _) = test_app_with_sessions(1);
    app.active_panel = Panel::Messages;

    app.focus_sessions();

    assert_eq!(app.active_panel, Panel::Sessions);
}

#[test]
fn test_focus_sessions_preserves_existing_selection() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(1);

    app.focus_sessions();

    assert_eq!(app.active_panel, Panel::Sessions);
    assert_eq!(app.selected_session, Some(keys[1].clone()));
}

#[test]
fn test_focus_sessions_empty_no_panic() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.focus_sessions();
    assert_eq!(app.active_panel, Panel::Sessions);
    assert!(app.selected_session.is_none());
}

// ── Cache message ──────────────────────────────────────────────

#[test]
fn test_cache_message_creates_new_session_entry() {
    let mut app = App::new(AppMode::Normal, None, None);
    let sk = SessionKey::dm(Platform::Discord, "new-user");

    app.cache_message(MessageEntry {
        session_key: sk.clone(),
        author: "alice".into(),
        content: "hi".into(),
    });

    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.sessions[0].session_key, sk);
    // Auto-selects when no previous selection
    assert_eq!(app.selected_session, Some(sk));
}

#[test]
fn test_cache_message_promotes_session_to_front() {
    let (mut app, keys) = test_app_with_sessions(3);
    app.select_session(0);

    app.cache_message(MessageEntry {
        session_key: keys[2].clone(),
        author: "bob".into(),
        content: "hey".into(),
    });

    assert_eq!(app.sessions[0].session_key, keys[2]);
}

#[test]
fn test_cache_message_sets_active_team() {
    let mut app = App::new(AppMode::Normal, None, None);
    let sk = SessionKey::dm(Platform::Discord, "user");
    app.active_teams.insert(sk.clone(), "team-a".into());
    app.sessions.push(session_entry(sk.clone()));

    app.cache_message(MessageEntry {
        session_key: sk.clone(),
        author: "alice".into(),
        content: "msg".into(),
    });

    assert_eq!(app.sessions[0].active_team.as_deref(), Some("team-a"));
}

#[test]
fn test_cache_message_does_not_update_messages_for_different_session() {
    let (mut app, keys) = test_app_with_sessions(2);
    app.select_session(0);
    let initial_msg_count = app.messages.len();

    app.cache_message(MessageEntry {
        session_key: keys[1].clone(),
        author: "bob".into(),
        content: "hello".into(),
    });

    // Messages display should not change since we're viewing keys[0]
    assert_eq!(app.messages.len(), initial_msg_count);
    // But the message should be cached
    assert_eq!(app.session_messages.get(&keys[1]).unwrap().len(), 1);
}

// ── Clear messages ─────────────────────────────────────────────

#[test]
fn test_clear_messages_with_selection_only_clears_selected() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.session_messages.insert(
        first.clone(),
        VecDeque::from([MessageEntry {
            session_key: first.clone(),
            author: "a".into(),
            content: "one".into(),
        }]),
    );
    app.session_messages.insert(
        second.clone(),
        VecDeque::from([MessageEntry {
            session_key: second.clone(),
            author: "b".into(),
            content: "two".into(),
        }]),
    );
    app.selected_session = Some(first.clone());

    app.clear_messages();

    // Only first session's messages are cleared
    assert!(!app.session_messages.contains_key(&first));
    assert!(app.session_messages.contains_key(&second));
    assert!(app.messages.is_empty());
    assert_eq!(app.messages_scroll, 0);
}

// ── Scroll visibility ──────────────────────────────────────────

#[test]
fn test_ensure_selected_session_visible_scrolls_down() {
    let (mut app, _) = test_app_with_sessions(10);
    app.sessions_area_height = 3;
    app.sessions_scroll = 0;

    // Select item at index 5, which is below visible area (0..2)
    app.select_session(5);

    // Should scroll down so index 5 is visible
    assert!(app.sessions_scroll > 0);
    let last_visible = app.sessions_scroll + app.sessions_area_height - 1;
    assert!(app.selected_session_index <= last_visible);
}

#[test]
fn test_ensure_selected_session_visible_scrolls_up() {
    let (mut app, _) = test_app_with_sessions(10);
    app.sessions_area_height = 3;
    app.sessions_scroll = 5;
    app.selected_session_index = 5;

    // Select item at index 2, which is above visible area
    app.select_session(2);

    assert!(app.sessions_scroll <= app.selected_session_index);
}

#[test]
fn test_ensure_selected_session_visible_zero_height_is_noop() {
    let (mut app, _) = test_app_with_sessions(5);
    app.sessions_area_height = 0;
    app.sessions_scroll = 3;

    app.select_session(0);

    // With zero height, scroll should not change
    assert_eq!(app.sessions_scroll, 3);
}

// ── Refresh sessions ───────────────────────────────────────────

#[test]
fn test_refresh_sessions_empty_clears_state() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.selected_session = Some(SessionKey::dm(Platform::Discord, "old"));
    app.messages.push_back(MessageEntry {
        session_key: SessionKey::dm(Platform::Discord, "old"),
        author: "a".into(),
        content: "c".into(),
    });

    app.refresh_sessions();

    assert!(app.selected_session.is_none());
    assert_eq!(app.selected_session_index, 0);
    assert!(app.messages.is_empty());
}

#[test]
fn test_refresh_sessions_sorts_active_first() {
    let mut app = App::new(AppMode::Normal, None, None);
    let inactive = SessionKey::dm(Platform::Discord, "inactive");
    let active = SessionKey::dm(Platform::Discord, "active");
    app.sessions.push(session_entry(inactive.clone()));
    app.sessions.push(session_entry(active.clone()));
    app.active_sessions.insert(active.clone());

    app.refresh_sessions();

    assert_eq!(app.sessions[0].session_key, active);
    assert!(app.sessions[0].is_active);
}

#[test]
fn test_refresh_sessions_adds_active_sessions_not_in_store() {
    let mut app = App::new(AppMode::Normal, None, None);
    let sk = SessionKey::dm(Platform::Discord, "ephemeral");
    app.active_sessions.insert(sk.clone());

    app.refresh_sessions();

    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.sessions[0].session_key, sk);
    assert!(app.sessions[0].is_active);
}

#[test]
fn test_attach_session_store_preserves_selection_and_maps_missing_authors() {
    let (store, _dir, path) = file_backed_session_store();
    let selected = SessionKey::dm(Platform::Discord, "selected");
    let other = SessionKey::dm(Platform::Discord, "other");
    store.append_user_message(&selected, "hello", None).unwrap();
    store.append_assistant_message(&selected, "reply").unwrap();
    store
        .append_user_message(&other, "later", Some("bob"))
        .unwrap();

    execute_sql(
        &path,
        &format!(
            "UPDATE messages SET author = NULL WHERE session_key = '{}' AND role = 'assistant'",
            selected.to_stable_id()
        ),
    );
    execute_sql(
        &path,
        &format!(
            "UPDATE sessions SET created_at = '2026-03-11 08:00:00', updated_at = '2026-03-11 08:00:00' WHERE session_key = '{}'",
            selected.to_stable_id()
        ),
    );
    execute_sql(
        &path,
        &format!(
            "UPDATE sessions SET created_at = '2026-03-11 09:00:00', updated_at = '2026-03-11 09:00:00' WHERE session_key = '{}'",
            other.to_stable_id()
        ),
    );

    let mut app = App::new(AppMode::Normal, None, None);
    app.active_teams.insert(selected.clone(), "team-a".into());
    app.selected_session = Some(selected.clone());

    app.attach_session_store(store);

    let selected_entry = app
        .sessions
        .iter()
        .find(|entry| entry.session_key == selected)
        .unwrap();
    assert_eq!(app.selected_session, Some(selected.clone()));
    assert_eq!(app.selected_session_index, 1);
    assert_eq!(selected_entry.active_team.as_deref(), Some("team-a"));
    assert_eq!(app.messages.len(), 2);
    assert_eq!(app.messages[0].author, "user");
    assert_eq!(app.messages[1].author, "goose");
}

#[test]
fn test_refresh_sessions_preserves_visible_messages_for_selected_session() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(SessionStore::new(db));
    let selected = SessionKey::dm(Platform::Discord, "cached");
    store
        .append_user_message(&selected, "persisted", Some("alice"))
        .unwrap();

    let mut app = App::new(AppMode::Normal, None, None);
    app.session_store = Some(store);
    app.sessions.push(session_entry(selected.clone()));
    app.selected_session = Some(selected.clone());
    app.messages.push_back(MessageEntry {
        session_key: selected.clone(),
        author: "cached".into(),
        content: "already visible".into(),
    });

    app.refresh_sessions();

    assert_eq!(app.selected_session, Some(selected));
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0].content, "already visible");
}

#[test]
fn test_attach_session_store_reports_list_sessions_errors() {
    let (store, _dir, path) = file_backed_session_store();
    execute_sql(&path, "ALTER TABLE sessions RENAME TO broken_sessions");

    let mut app = App::new(AppMode::Normal, None, None);
    app.attach_session_store(store);

    assert!(
        app.events
            .back()
            .unwrap()
            .summary
            .contains("Could not load session history")
    );
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
    assert!(
        app.status_notice
            .as_ref()
            .unwrap()
            .message
            .contains("Could not load session history")
    );
}

#[test]
fn test_select_session_reports_history_load_errors() {
    let (store, _dir, path) = file_backed_session_store();
    let selected = SessionKey::dm(Platform::Discord, "broken-history");
    execute_sql(&path, "ALTER TABLE messages RENAME TO broken_messages");

    let mut app = App::new(AppMode::Normal, None, None);
    app.session_store = Some(store);
    app.sessions.push(session_entry(selected.clone()));

    app.select_session(0);

    assert_eq!(app.selected_session, Some(selected.clone()));
    assert!(app.messages.is_empty());
    assert!(app.events.back().unwrap().summary.contains(&format!(
        "Could not load history for {}",
        App::format_session_label(&selected)
    )));
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
}
