use std::time::Instant;

use opengoose_types::{Platform, SessionKey};
use tokio::sync::mpsc;

use super::*;

#[test]
fn test_secret_input_state_defaults() {
    let s = SecretInputState::new();
    assert!(!s.visible);
    assert!(s.input.is_empty());
    assert!(s.status_message.is_none());
    assert!(s.title.is_none());
    assert!(s.is_secret);
}

#[test]
fn test_composer_state_editing_clears_history_navigation() {
    let mut composer = ComposerState::new();
    composer.push_history("alpha".into());
    composer.push_history("beta".into());
    composer.history_previous();
    assert_eq!(composer.input, "beta");

    composer.insert_char('!');

    assert_eq!(composer.input, "beta!");
    assert!(composer.history_index.is_none());
    assert!(composer.history_draft.is_none());
}

#[test]
fn test_command_palette_state_defaults() {
    let cp = CommandPaletteState::new();
    assert!(!cp.visible);
    assert!(cp.input.is_empty());
    assert_eq!(cp.selected, 0);
}

#[test]
fn test_provider_select_state_defaults() {
    let ps = ProviderSelectState::new();
    assert!(!ps.visible);
    assert!(ps.providers.is_empty());
    assert!(ps.provider_ids.is_empty());
    assert_eq!(ps.selected, 0);
    assert_eq!(ps.purpose, ProviderSelectPurpose::Configure);
}

#[test]
fn test_credential_flow_reset() {
    let mut cf = CredentialFlowState::new();
    cf.provider_id = Some("openai".into());
    cf.provider_display = Some("OpenAI".into());
    cf.current_key = 1;
    cf.collected.push(("KEY".into(), "val".into()));

    cf.reset();
    assert!(cf.provider_id.is_none());
    assert!(cf.provider_display.is_none());
    assert_eq!(cf.current_key, 0);
    assert!(cf.collected.is_empty());
}

#[test]
fn test_credential_flow_state_defaults() {
    let cf = CredentialFlowState::new();
    assert!(cf.provider_id.is_none());
    assert!(cf.provider_display.is_none());
    assert!(cf.keys.is_empty());
    assert_eq!(cf.current_key, 0);
    assert!(cf.collected.is_empty());
}

#[test]
fn test_credential_flow_current_empty() {
    let cf = CredentialFlowState::new();
    assert!(cf.current().is_none());
}

#[test]
fn test_credential_flow_current_with_keys() {
    let mut cf = CredentialFlowState::new();
    cf.keys.push(CredentialKey {
        env_var: "API_KEY".into(),
        label: "API Key".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    assert!(cf.current().is_some());
    assert_eq!(cf.current().unwrap().env_var, "API_KEY");
}

#[test]
fn test_credential_flow_has_more() {
    let mut cf = CredentialFlowState::new();
    assert!(!cf.has_more());

    cf.keys.push(CredentialKey {
        env_var: "KEY1".into(),
        label: "Key 1".into(),
        secret: false,
        oauth_flow: false,
        required: true,
        default: None,
    });
    assert!(!cf.has_more());

    cf.keys.push(CredentialKey {
        env_var: "KEY2".into(),
        label: "Key 2".into(),
        secret: false,
        oauth_flow: false,
        required: true,
        default: None,
    });
    assert!(cf.has_more());

    cf.current_key = 1;
    assert!(!cf.has_more());
}

#[test]
fn test_clear_events() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.events.push_back(EventEntry {
        summary: "test".into(),
        level: EventLevel::Info,
        timestamp: Instant::now(),
    });
    app.events_scroll = 3;
    app.clear_events();
    assert!(app.events.is_empty());
    assert_eq!(app.events_scroll, 0);
}

#[test]
fn test_events_line_count_nonempty() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.events.push_back(EventEntry {
        summary: "a".into(),
        level: EventLevel::Info,
        timestamp: Instant::now(),
    });
    app.events.push_back(EventEntry {
        summary: "b".into(),
        level: EventLevel::Error,
        timestamp: Instant::now(),
    });
    assert_eq!(app.events_line_count(), 2);
}

#[test]
fn test_model_select_state_defaults() {
    let ms = ModelSelectState::new();
    assert!(!ms.visible);
    assert!(ms.models.is_empty());
    assert_eq!(ms.selected, 0);
    assert!(!ms.loading);
    assert!(ms.provider_name.is_empty());
}

#[test]
fn test_app_new_defaults() {
    let app = App::new(AppMode::Normal, None, None);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.messages.is_empty());
    assert!(app.events.is_empty());
    assert!(app.sessions.is_empty());
    assert_eq!(app.active_panel, Panel::Messages);
    assert_eq!(app.messages_scroll, 0);
    assert_eq!(app.sessions_scroll, 0);
    assert_eq!(app.agent_status, AgentStatus::Idle);
    assert!(!app.should_quit);
    assert!(app.pairing_code.is_none());
    assert!(app.connected_platforms.is_empty());
    assert!(app.active_sessions.is_empty());
    assert!(app.active_teams.is_empty());
    assert!(app.cached_providers.is_empty());
    assert_eq!(
        app.composer_session_key(),
        SessionKey::direct(Platform::Custom("tui".into()), "local")
    );
}

#[test]
fn test_submit_composer_uses_local_session_when_none_selected() {
    let mut app = App::new(AppMode::Normal, None, None);
    let (tx, mut rx) = mpsc::unbounded_channel();
    app.set_composer_tx(tx);
    app.composer.input = "hello".into();
    app.composer.cursor = 5;

    app.submit_composer();

    let request = rx.try_recv().unwrap();
    assert_eq!(
        request.session_key,
        SessionKey::direct(Platform::Custom("tui".into()), "local")
    );
    assert_eq!(request.content, "hello");
    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer.history.back().map(String::as_str),
        Some("hello")
    );
}

#[test]
fn test_cache_message_syncs_selected_session() {
    let mut app = App::new(AppMode::Normal, None, None);
    let session_key = SessionKey::direct(Platform::Discord, "dm-1");
    app.sessions.push(SessionListEntry {
        session_key: session_key.clone(),
        active_team: None,
        created_at: None,
        updated_at: None,
        is_active: true,
    });
    app.select_session(0);

    app.cache_message(MessageEntry {
        session_key: session_key.clone(),
        author: "alice".into(),
        content: "hello".into(),
    });

    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages.back().unwrap().content, "hello");
    assert_eq!(app.selected_session, Some(session_key));
}

#[test]
fn test_clear_messages_clears_selected_cache() {
    let mut app = App::new(AppMode::Normal, None, None);
    let session_key = SessionKey::direct(Platform::Discord, "dm-1");
    app.sessions.push(SessionListEntry {
        session_key: session_key.clone(),
        active_team: None,
        created_at: None,
        updated_at: None,
        is_active: true,
    });
    app.select_session(0);
    app.cache_message(MessageEntry {
        session_key,
        author: "alice".into(),
        content: "hello".into(),
    });

    app.clear_messages();

    assert!(app.messages.is_empty());
    assert!(app.session_messages.is_empty());
}

#[test]
fn test_events_line_count_empty() {
    let app = App::new(AppMode::Normal, None, None);
    assert_eq!(app.events_line_count(), 1);
}

#[test]
fn test_format_session_label_direct() {
    let session_key = SessionKey::direct(Platform::Slack, "ops");
    assert_eq!(App::format_session_label(&session_key), "slack:ops");
}

#[test]
fn test_format_session_label_namespaced() {
    let session_key = SessionKey::new(Platform::Discord, "guild", "thread");
    assert_eq!(
        App::format_session_label(&session_key),
        "discord:guild/thread"
    );
}

#[test]
fn test_initialize_runtime_state_handles_db_open() {
    let mut app = App::new(AppMode::Normal, None, None);

    app.initialize_runtime_state();

    if app.session_store.is_none() {
        let notice = app.status_notice.as_ref().expect("db open failure should set notice");
        assert_eq!(notice.level, EventLevel::Error);
        assert!(
            notice.message.starts_with("Session history is unavailable:")
        );
    } else {
        assert!(app.status_notice.is_none());
    }
}

#[test]
fn test_messages_line_count_for_multi_line_messages() {
    let mut app = App::new(AppMode::Normal, None, None);
    app.messages_area_width = 20;
    app.messages.push_back(MessageEntry {
        session_key: SessionKey::direct(Platform::Slack, "ops"),
        author: "alice".into(),
        content: "abcdefghijklmnopqrstuvwxy".into(),
    });

    assert!(app.messages_line_count() >= 3);
}
