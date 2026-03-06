mod credential_flow;
mod event_handler;
mod state;

pub use state::*;

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_secrets::{ConfigFile, SecretResult, SecretStore, SecretValue};
    use opengoose_types::{AppEvent, AppEventKind, Platform, SessionKey};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    struct MockStore {
        secrets: Mutex<HashMap<String, String>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                secrets: Mutex::new(HashMap::new()),
            }
        }
    }

    impl SecretStore for MockStore {
        fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(self
                .secrets
                .lock()
                .unwrap()
                .get(key)
                .map(|v| SecretValue::new(v.clone())))
        }
        fn set(&self, key: &str, value: &str) -> SecretResult<()> {
            self.secrets
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> SecretResult<bool> {
            Ok(self.secrets.lock().unwrap().remove(key).is_some())
        }
    }

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    fn make_event(kind: AppEventKind) -> AppEvent {
        AppEvent {
            kind,
            timestamp: Instant::now(),
        }
    }

    #[test]
    fn test_push_event_buffer_limit() {
        let mut app = test_app();
        for i in 0..MAX_EVENTS + 10 {
            app.push_event(&format!("event {i}"), EventLevel::Info);
        }
        assert_eq!(app.events.len(), MAX_EVENTS);
        // Oldest events should be dropped — first remaining is "event 10"
        assert_eq!(app.events.front().unwrap().summary, "event 10");
    }

    #[test]
    fn test_handle_discord_ready() {
        let mut app = test_app();
        assert!(!app.connected_platforms.contains(&Platform::Discord));
        app.handle_app_event(make_event(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        }));
        assert!(app.connected_platforms.contains(&Platform::Discord));
    }

    #[test]
    fn test_handle_discord_disconnected() {
        let mut app = test_app();
        app.connected_platforms.insert(Platform::Discord);
        app.handle_app_event(make_event(AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: "test".into(),
        }));
        assert!(!app.connected_platforms.contains(&Platform::Discord));
    }

    #[test]
    fn test_handle_message_received() {
        let mut app = test_app();
        app.messages_scroll = 5;
        let sk = SessionKey::dm(Platform::Discord, "user1");
        app.handle_app_event(make_event(AppEventKind::MessageReceived {
            session_key: sk.clone(),
            author: "alice".into(),
            content: "hello".into(),
        }));
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages.back().unwrap().author, "alice");
        assert_eq!(app.messages.back().unwrap().content, "hello");
        assert_eq!(app.messages_scroll, 0); // scroll resets
    }

    #[test]
    fn test_handle_response_sent() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "user1");
        app.handle_app_event(make_event(AppEventKind::ResponseSent {
            session_key: sk,
            content: "hi there".into(),
        }));
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages.back().unwrap().author, "goose");
    }

    #[test]
    fn test_message_buffer_limit() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "user1");
        for i in 0..MAX_MESSAGES + 10 {
            app.handle_app_event(make_event(AppEventKind::MessageReceived {
                session_key: sk.clone(),
                author: "user".into(),
                content: format!("msg {i}"),
            }));
        }
        assert_eq!(app.messages.len(), MAX_MESSAGES);
    }

    #[test]
    fn test_handle_pairing_code() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::PairingCodeGenerated {
            code: "ABC123".into(),
        }));
        assert_eq!(app.pairing_code, Some("ABC123".into()));
    }

    #[test]
    fn test_handle_pairing_completed() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "user1");
        app.handle_app_event(make_event(AppEventKind::PairingCompleted {
            session_key: sk.clone(),
        }));
        assert!(app.active_sessions.contains(&sk));
    }

    #[test]
    fn test_handle_session_disconnected() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "user1");
        app.active_sessions.insert(sk.clone());
        app.handle_app_event(make_event(AppEventKind::SessionDisconnected {
            session_key: sk.clone(),
            reason: "left".into(),
        }));
        assert!(!app.active_sessions.contains(&sk));
    }

    #[test]
    fn test_clear_messages_and_events() {
        let mut app = test_app();
        app.push_event("test event", EventLevel::Info);
        app.messages.push_back(MessageEntry {
            session_key: SessionKey::dm(Platform::Discord, "u"),
            author: "a".into(),
            content: "c".into(),
        });
        app.messages_scroll = 5;
        app.events_scroll = 3;

        app.clear_messages();
        assert!(app.messages.is_empty());
        assert_eq!(app.messages_scroll, 0);

        app.clear_events();
        assert!(app.events.is_empty());
        assert_eq!(app.events_scroll, 0);
    }

    #[test]
    fn test_events_line_count_empty() {
        let app = test_app();
        assert_eq!(app.events_line_count(), 1); // empty returns 1
    }

    #[test]
    fn test_events_line_count_with_events() {
        let mut app = test_app();
        app.push_event("a", EventLevel::Info);
        app.push_event("b", EventLevel::Error);
        app.push_event("c", EventLevel::Info);
        assert_eq!(app.events_line_count(), 3);
    }

    #[test]
    fn test_handle_error_event_goes_to_events() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::Error {
            context: "test".into(),
            message: "something went wrong".into(),
        }));
        // Error events go to events panel with Error level
        assert_eq!(app.events.len(), 1);
        assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
    }

    #[test]
    fn test_handle_tracing_event_goes_to_events() {
        let mut app = test_app();
        app.handle_app_event(make_event(AppEventKind::TracingEvent {
            level: "INFO".into(),
            message: "trace msg".into(),
        }));
        assert_eq!(app.events.len(), 1);
        assert_eq!(app.events.back().unwrap().level, EventLevel::Info);
    }

    #[test]
    fn test_message_events_not_in_events_panel() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "u");
        app.handle_app_event(make_event(AppEventKind::MessageReceived {
            session_key: sk.clone(),
            author: "alice".into(),
            content: "hi".into(),
        }));
        // MessageReceived should NOT add to events panel
        assert_eq!(app.events.len(), 0);
        assert_eq!(app.messages.len(), 1);
    }

    #[test]
    fn test_response_sent_not_in_events_panel() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "u");
        app.handle_app_event(make_event(AppEventKind::ResponseSent {
            session_key: sk,
            content: "reply".into(),
        }));
        assert_eq!(app.events.len(), 0);
        assert_eq!(app.messages.len(), 1);
    }

    #[test]
    fn test_new_setup_mode() {
        let app = App::new(AppMode::Setup, None, None);
        assert_eq!(app.mode, AppMode::Setup);
        assert!(!app.connected_platforms.contains(&Platform::Discord));
        assert!(app.messages.is_empty());
        assert!(app.events.is_empty());
    }

    #[test]
    fn test_save_secret_empty_token() {
        let mut app = test_app();
        app.secret_input.visible = true;
        app.secret_input.input.clear();
        // Should set error status, not panic
        let result = app.save_secret_and_notify();
        assert!(result.is_ok());
        assert_eq!(
            app.secret_input.status_message,
            Some("Token cannot be empty".into())
        );
    }

    #[test]
    fn test_tick_no_panic() {
        let mut app = test_app();
        app.tick(); // Should not panic
    }

    #[test]
    fn test_push_event_levels() {
        let mut app = test_app();
        app.push_event("info msg", EventLevel::Info);
        app.push_event("error msg", EventLevel::Error);
        assert_eq!(app.events[0].level, EventLevel::Info);
        assert_eq!(app.events[1].level, EventLevel::Error);
    }

    #[test]
    fn test_response_sent_buffer_limit() {
        let mut app = test_app();
        let sk = SessionKey::dm(Platform::Discord, "user1");
        for i in 0..MAX_MESSAGES + 5 {
            app.handle_app_event(make_event(AppEventKind::ResponseSent {
                session_key: sk.clone(),
                content: format!("resp {i}"),
            }));
        }
        assert_eq!(app.messages.len(), MAX_MESSAGES);
    }

    #[test]
    fn test_handle_app_event_events_buffer_limit() {
        let mut app = test_app();
        // Fill events to MAX_EVENTS via handle_app_event (non-message events)
        for i in 0..MAX_EVENTS + 5 {
            app.handle_app_event(make_event(AppEventKind::Error {
                context: "test".into(),
                message: format!("err {i}"),
            }));
        }
        assert_eq!(app.events.len(), MAX_EVENTS);
    }

    // ── save_secret_and_notify with mock store ──────────────

    #[test]
    fn test_save_secret_stores_in_keyring_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let store = Arc::new(MockStore::new());

        let mut app = App::with_store(
            AppMode::Normal,
            None,
            None,
            store.clone(),
            Some(config_path.clone()),
        );
        app.secret_input.visible = true;
        app.secret_input.input = "my_bot_token".into();

        let result = app.save_secret_and_notify();
        assert!(result.is_ok());

        // Token should be stored in mock keyring
        assert_eq!(
            store.get("discord_bot_token").unwrap().unwrap().as_str(),
            "my_bot_token"
        );

        // Config should have marked in_keyring
        let loaded = ConfigFile::load_from(&config_path).unwrap();
        assert!(loaded.secrets.get("discord_bot_token").unwrap().in_keyring);

        // UI state should be reset
        assert!(!app.secret_input.visible);
        assert!(app.secret_input.input.is_empty());
        assert!(app.secret_input.status_message.is_none());

        // Should push event since no token_sender
        assert_eq!(app.events.len(), 1);
        assert!(app.events.back().unwrap().summary.contains("Token updated"));
    }

    #[test]
    fn test_save_secret_with_token_sender_switches_to_normal() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let (tx, mut rx) = tokio::sync::oneshot::channel();

        let mut app = App::with_store(
            AppMode::Setup,
            Some(tx),
            None,
            Arc::new(MockStore::new()),
            Some(config_path),
        );
        app.secret_input.visible = true;
        app.secret_input.input = "setup_token".into();

        let result = app.save_secret_and_notify();
        assert!(result.is_ok());
        assert_eq!(app.mode, AppMode::Normal);

        // Token should have been sent via oneshot
        let received = rx.try_recv().unwrap();
        assert_eq!(received, "setup_token");

        // No event pushed in setup mode transition
        assert_eq!(app.events.len(), 0);
    }
}
