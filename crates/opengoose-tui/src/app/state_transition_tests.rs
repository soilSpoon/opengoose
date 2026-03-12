use std::collections::VecDeque;
use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_persistence::SessionStore;
use opengoose_types::{Platform, SessionKey};
use tokio::sync::mpsc;

use super::session_types::MAX_MESSAGES;
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

#[test]
fn test_submit_composer_uses_selected_session() {
    let mut app = App::new(AppMode::Normal, None, None);
    let session_key = SessionKey::dm(Platform::Discord, "dm-1");
    let (tx, mut rx) = mpsc::unbounded_channel();
    app.set_composer_tx(tx);
    app.selected_session = Some(session_key.clone());
    app.composer.input = "reply".into();

    app.submit_composer();

    let request = rx.try_recv().unwrap();
    assert_eq!(request.session_key, session_key);
    assert_eq!(request.content, "reply");
    assert!(app.composer.input.is_empty());
}

#[test]
fn test_submit_composer_without_sender_records_error() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.composer.input = "hello".into();

    app.submit_composer();

    assert_eq!(
        app.events.back().unwrap().summary,
        "Message sending is unavailable in the current TUI mode."
    );
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
}

#[test]
fn test_select_next_session_clamps_to_last() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.sessions.push(session_entry(first));
    app.sessions.push(session_entry(second));
    app.selected_session = Some(SessionKey::dm(Platform::Discord, "dm-1"));
    app.selected_session_index = 0;

    app.select_next_session();

    assert_eq!(app.selected_session_index, 1);
    assert_eq!(
        app.selected_session,
        Some(SessionKey::dm(Platform::Discord, "dm-2"))
    );
}

#[test]
fn test_select_next_session_stays_last_when_at_end() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.sessions.push(session_entry(first));
    app.sessions.push(session_entry(second));
    app.select_last_session();

    app.select_next_session();

    assert_eq!(app.selected_session_index, 1);
    assert_eq!(
        app.selected_session,
        Some(SessionKey::dm(Platform::Discord, "dm-2"))
    );
}

#[test]
fn test_select_previous_session_stays_first_when_at_beginning() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.sessions.push(session_entry(first.clone()));
    app.sessions.push(session_entry(second));
    app.select_first_session();

    app.select_previous_session();

    assert_eq!(app.selected_session_index, 0);
    assert_eq!(app.selected_session, Some(first));
}

#[test]
fn test_clear_messages_with_selected_session_only_clears_active_cache() {
    let mut app = App::new(AppMode::Normal, None, None);
    let selected = SessionKey::dm(Platform::Discord, "selected");
    let other = SessionKey::dm(Platform::Discord, "other");
    app.session_messages.insert(
        selected.clone(),
        VecDeque::from([MessageEntry {
            session_key: selected.clone(),
            author: "me".into(),
            content: "keep".into(),
        }]),
    );
    app.session_messages.insert(
        other.clone(),
        VecDeque::from([MessageEntry {
            session_key: other.clone(),
            author: "agent".into(),
            content: "other".into(),
        }]),
    );
    app.selected_session = Some(selected.clone());

    app.clear_messages();

    assert!(!app.session_messages.contains_key(&selected));
    assert!(app.session_messages.contains_key(&other));
}

#[test]
fn test_submit_composer_send_failure_records_error() {
    let mut app = App::new(AppMode::Normal, None, None);
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);
    app.set_composer_tx(tx);
    app.composer.input = "hello".into();

    app.submit_composer();

    assert_eq!(
        app.events.back().unwrap().summary,
        "Failed to submit the message to the local engine."
    );
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
}

#[test]
fn test_clear_messages_without_selection_clears_all_cached_sessions() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.session_messages.insert(
        first.clone(),
        VecDeque::from([MessageEntry {
            session_key: first,
            author: "alice".into(),
            content: "one".into(),
        }]),
    );
    app.session_messages.insert(
        second.clone(),
        VecDeque::from([MessageEntry {
            session_key: second,
            author: "bob".into(),
            content: "two".into(),
        }]),
    );

    app.clear_messages();

    assert!(app.messages.is_empty());
    assert!(app.session_messages.is_empty());
}

#[test]
fn test_focus_sessions_selects_first_session_and_loads_cached_messages() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    app.sessions.push(session_entry(first.clone()));
    app.sessions
        .push(session_entry(SessionKey::dm(Platform::Discord, "dm-2")));
    app.session_messages.insert(
        first.clone(),
        VecDeque::from([MessageEntry {
            session_key: first.clone(),
            author: "alice".into(),
            content: "hello".into(),
        }]),
    );

    app.focus_sessions();

    assert_eq!(app.active_panel, Panel::Sessions);
    assert_eq!(app.selected_session, Some(first));
    assert_eq!(app.messages.back().unwrap().content, "hello");
}

#[test]
fn test_refresh_sessions_preserves_selection_and_updates_scroll() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.sessions = vec![
        SessionListEntry {
            updated_at: Some("2024-01-01T00:00:00Z".into()),
            ..session_entry(second.clone())
        },
        SessionListEntry {
            updated_at: Some("2024-01-02T00:00:00Z".into()),
            ..session_entry(first.clone())
        },
    ];
    app.selected_session = Some(second.clone());
    app.sessions_area_height = 1;

    app.refresh_sessions();

    assert_eq!(app.sessions[0].session_key, first);
    assert_eq!(app.selected_session, Some(second));
    assert_eq!(app.selected_session_index, 1);
    assert_eq!(app.sessions_scroll, 1);
}

#[test]
fn test_attach_session_store_loads_sessions_and_history() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(SessionStore::new(db));
    let session_key = SessionKey::dm(Platform::Discord, "dm-1");
    store
        .append_user_message(&session_key, "hello", Some("alice"))
        .unwrap();
    store
        .append_assistant_message(&session_key, "hi there")
        .unwrap();
    store.set_active_team(&session_key, Some("ops")).unwrap();

    let mut app = App::new(AppMode::Normal, None, None);
    app.active_sessions.insert(session_key.clone());

    app.attach_session_store(store);

    assert_eq!(app.selected_session, Some(session_key));
    assert_eq!(app.sessions[0].active_team.as_deref(), Some("ops"));
    assert_eq!(app.messages[0].author, "alice");
    assert_eq!(app.messages[1].author, "goose");
}

#[test]
fn test_cache_message_promotes_existing_session_and_enforces_limit() {
    let mut app = App::new(AppMode::Normal, None, None);
    let first = SessionKey::dm(Platform::Discord, "dm-1");
    let second = SessionKey::dm(Platform::Discord, "dm-2");
    app.sessions.push(session_entry(first));
    app.sessions.push(session_entry(second.clone()));
    app.selected_session = Some(second.clone());
    app.selected_session_index = 1;
    app.active_sessions.insert(second.clone());
    app.active_teams.insert(second.clone(), "ops".into());

    for i in 0..=MAX_MESSAGES {
        app.cache_message(MessageEntry {
            session_key: second.clone(),
            author: "alice".into(),
            content: format!("msg {i}"),
        });
    }

    let cached = app.session_messages.get(&second).unwrap();
    assert_eq!(app.sessions[0].session_key, second);
    assert!(app.sessions[0].is_active);
    assert_eq!(app.sessions[0].active_team.as_deref(), Some("ops"));
    assert_eq!(cached.len(), MAX_MESSAGES);
    assert_eq!(cached.front().unwrap().content, "msg 1");
}

#[test]
fn test_request_new_session_sends_pairing_signal() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(AppMode::Normal, None, Some(tx));

    app.request_new_session();

    assert!(rx.try_recv().is_ok());
    assert_eq!(app.events.back().unwrap().level, EventLevel::Info);
}

#[test]
fn test_request_new_session_without_pairing_sender_records_error() {
    let mut app = App::new(AppMode::Normal, None, None);

    app.request_new_session();

    assert_eq!(
        app.events.back().unwrap().summary,
        "New sessions are not available in this mode."
    );
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
}

#[test]
fn test_set_status_notice_records_message_and_level() {
    let mut app = App::new(AppMode::Normal, None, None);

    app.set_status_notice("Heads up".into(), EventLevel::Error);

    let notice = app.status_notice.as_ref().unwrap();
    assert_eq!(notice.message, "Heads up");
    assert_eq!(notice.level, EventLevel::Error);
}

#[test]
fn test_set_agent_status_tracks_active_session() {
    let mut app = App::new(AppMode::Normal, None, None);
    let session_key = SessionKey::dm(Platform::Discord, "dm-1");

    app.set_agent_status(AgentStatus::Generating, Some(session_key.clone()));

    assert_eq!(app.agent_status, AgentStatus::Generating);
    assert_eq!(app.agent_status_session, Some(session_key));
}
