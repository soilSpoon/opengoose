use opengoose_types::{AppEventKind, Platform, SessionKey};

use super::super::state::*;
use super::tests_support::test_app;

#[test]
fn test_apply_channel_ready_and_disconnected_updates_connected_platforms() {
    let mut app = test_app();
    let platform = Platform::Discord;

    super::reducer::apply(
        &mut app,
        &AppEventKind::ChannelReady {
            platform: platform.clone(),
        },
    );

    assert!(app.connected_platforms.contains(&platform));

    super::reducer::apply(
        &mut app,
        &AppEventKind::ChannelDisconnected {
            platform: platform.clone(),
            reason: "network down".into(),
        },
    );

    assert!(!app.connected_platforms.contains(&platform));
}

#[test]
fn test_apply_message_and_response_events_update_messages_and_select_session() {
    let mut app = test_app();
    let message_session = SessionKey::dm(Platform::Discord, "user-1");

    super::reducer::apply(
        &mut app,
        &AppEventKind::MessageReceived {
            session_key: message_session.clone(),
            author: "alice".into(),
            content: "hello".into(),
        },
    );
    assert_eq!(app.selected_session, Some(message_session.clone()));
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages.back().unwrap().author, "alice");
    assert_eq!(app.messages.back().unwrap().content, "hello");

    super::reducer::apply(
        &mut app,
        &AppEventKind::ResponseSent {
            session_key: message_session.clone(),
            content: "ack".into(),
        },
    );
    assert_eq!(app.messages.len(), 2);
    assert_eq!(app.messages.back().unwrap().author, "goose");
    assert_eq!(app.messages.back().unwrap().content, "ack");
}

#[test]
fn test_apply_pairing_code_sets_state_and_sessions_update() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "session-1");

    super::reducer::apply(
        &mut app,
        &AppEventKind::PairingCodeGenerated {
            code: "abc123".into(),
        },
    );
    super::reducer::apply(
        &mut app,
        &AppEventKind::PairingCompleted {
            session_key: session_key.clone(),
        },
    );

    assert_eq!(app.pairing_code.as_deref(), Some("abc123"));
    assert!(app.active_sessions.contains(&session_key));
    assert!(!app.sessions.is_empty());
}

#[test]
fn test_apply_session_and_team_events_update_collections() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "session-team");

    app.active_sessions.insert(session_key.clone());
    super::reducer::apply(
        &mut app,
        &AppEventKind::SessionDisconnected {
            session_key: session_key.clone(),
            reason: "left".into(),
        },
    );
    assert!(!app.active_sessions.contains(&session_key));

    super::reducer::apply(
        &mut app,
        &AppEventKind::TeamActivated {
            session_key: session_key.clone(),
            team_name: "ops".into(),
        },
    );
    assert_eq!(
        app.active_teams.get(&session_key).map(String::as_str),
        Some("ops")
    );

    super::reducer::apply(
        &mut app,
        &AppEventKind::TeamDeactivated {
            session_key: session_key.clone(),
        },
    );
    assert!(!app.active_teams.contains_key(&session_key));
}

#[test]
fn test_apply_error_and_stream_events_update_agent_status() {
    let mut app = test_app();
    let session_key = SessionKey::dm(Platform::Discord, "agent");

    app.set_agent_status(AgentStatus::Generating, Some(session_key.clone()));
    super::reducer::apply(
        &mut app,
        &AppEventKind::Error {
            context: "sync".into(),
            message: "timed out".into(),
        },
    );
    assert_eq!(app.agent_status, AgentStatus::Idle);
    assert_eq!(app.agent_status_session, None);

    super::reducer::apply(
        &mut app,
        &AppEventKind::StreamStarted {
            session_key: session_key.clone(),
            stream_id: "s1".into(),
        },
    );
    assert_eq!(app.agent_status, AgentStatus::Thinking);
    assert_eq!(app.agent_status_session, Some(session_key.clone()));

    super::reducer::apply(
        &mut app,
        &AppEventKind::StreamUpdated {
            session_key: session_key.clone(),
            stream_id: "s1".into(),
            content_len: 32,
        },
    );
    assert_eq!(app.agent_status, AgentStatus::Generating);
    assert_eq!(app.agent_status_session, Some(session_key.clone()));

    super::reducer::apply(
        &mut app,
        &AppEventKind::StreamCompleted {
            session_key,
            stream_id: "s1".into(),
            full_text: "done".into(),
        },
    );
    assert_eq!(app.agent_status, AgentStatus::Idle);
}

#[test]
fn test_shows_in_messages_only_message_variants() {
    assert!(super::reducer::shows_in_messages(
        &AppEventKind::MessageReceived {
            session_key: SessionKey::dm(Platform::Discord, "user-1"),
            author: "alice".into(),
            content: "hello".into(),
        }
    ));
    assert!(super::reducer::shows_in_messages(
        &AppEventKind::ResponseSent {
            session_key: SessionKey::dm(Platform::Discord, "user-1"),
            content: "done".into(),
        }
    ));
    assert!(!super::reducer::shows_in_messages(&AppEventKind::Error {
        context: "x".into(),
        message: "y".into(),
    }));
}

#[test]
fn test_apply_noop_variant_keeps_state() {
    let mut app = test_app();
    let before_events = app.events.len();
    let before_sessions = app.sessions.len();

    super::reducer::apply(&mut app, &AppEventKind::DashboardUpdated);

    assert_eq!(app.events.len(), before_events);
    assert_eq!(app.sessions.len(), before_sessions);
    assert_eq!(app.connected_platforms.len(), 0);
}
