use opengoose_types::{AppEventKind, Platform, SessionKey};

use super::super::state::*;
use super::tests_support::{make_event, test_app};

#[test]
fn test_handle_message_received_caches_selected_messages() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "user1");
    app.sessions.push(SessionListEntry {
        session_key: session_key.clone(),
        active_team: None,
        created_at: None,
        updated_at: None,
        is_active: true,
    });
    app.select_session(0);

    app.handle_app_event(make_event(AppEventKind::MessageReceived {
        session_key: session_key.clone(),
        author: "alice".into(),
        content: "hello".into(),
    }));

    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages.back().unwrap().author, "alice");
    assert_eq!(app.selected_session, Some(session_key));
}

#[test]
fn test_handle_pairing_completed_refreshes_sessions() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "user1");

    app.handle_app_event(make_event(AppEventKind::PairingCompleted {
        session_key: session_key.clone(),
    }));

    assert!(app.active_sessions.contains(&session_key));
}

#[test]
fn test_handle_stream_events_update_agent_status() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "ch1");

    app.handle_app_event(make_event(AppEventKind::StreamStarted {
        session_key: session_key.clone(),
        stream_id: "s1".into(),
    }));
    assert_eq!(app.agent_status, AgentStatus::Thinking);

    app.handle_app_event(make_event(AppEventKind::StreamUpdated {
        session_key: session_key.clone(),
        stream_id: "s1".into(),
        content_len: 100,
    }));
    assert_eq!(app.agent_status, AgentStatus::Generating);

    app.handle_app_event(make_event(AppEventKind::StreamCompleted {
        session_key,
        stream_id: "s1".into(),
        full_text: "done".into(),
    }));
    assert_eq!(app.agent_status, AgentStatus::Idle);
}

#[test]
fn test_handle_error_sets_notice() {
    let mut app = test_app();

    app.handle_app_event(make_event(AppEventKind::Error {
        context: "relay".into(),
        message: "timed out while waiting".into(),
    }));

    assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
    assert!(
        app.status_notice
            .as_ref()
            .unwrap()
            .message
            .contains("timed out")
    );
}
