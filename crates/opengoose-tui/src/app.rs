use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use opengoose_secrets::{ConfigFile, SecretKey, SecretStore, default_store};
use opengoose_types::{AppEvent, AppEventKind, SessionKey};
use tokio::sync::{mpsc, oneshot};

const MAX_MESSAGES: usize = 1000;
const MAX_EVENTS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Setup,
    Normal,
}

#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub session_key: SessionKey,
    pub author: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct EventEntry {
    pub summary: String,
    pub level: EventLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    Info,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Messages,
    Events,
}

pub struct SecretInputState {
    pub visible: bool,
    pub input: String,
    pub status_message: Option<String>,
}

impl SecretInputState {
    fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            status_message: None,
        }
    }
}

pub struct CommandPaletteState {
    pub visible: bool,
    pub input: String,
    pub selected: usize,
}

impl CommandPaletteState {
    fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            selected: 0,
        }
    }
}

pub struct App {
    pub mode: AppMode,
    pub messages: VecDeque<MessageEntry>,
    pub events: VecDeque<EventEntry>,
    pub active_panel: Panel,
    pub messages_scroll: usize,
    pub events_scroll: usize,
    pub command_palette: CommandPaletteState,
    pub secret_input: SecretInputState,
    pub token_sender: Option<oneshot::Sender<String>>,
    pub pairing_tx: Option<mpsc::UnboundedSender<()>>,
    pub pairing_code: Option<String>,
    pub discord_connected: bool,
    pub active_sessions: HashSet<SessionKey>,
    pub messages_area_height: usize,
    pub events_area_height: usize,
    pub should_quit: bool,
    pub start_time: Instant,
    store: Arc<dyn SecretStore>,
    config_path: Option<PathBuf>,
}

impl App {
    pub fn new(
        mode: AppMode,
        token_sender: Option<oneshot::Sender<String>>,
        pairing_tx: Option<mpsc::UnboundedSender<()>>,
    ) -> Self {
        Self::with_store(mode, token_sender, pairing_tx, default_store(), None)
    }

    pub fn with_store(
        mode: AppMode,
        token_sender: Option<oneshot::Sender<String>>,
        pairing_tx: Option<mpsc::UnboundedSender<()>>,
        store: Arc<dyn SecretStore>,
        config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            mode,
            messages: VecDeque::new(),
            events: VecDeque::new(),
            active_panel: Panel::Messages,
            messages_scroll: 0,
            events_scroll: 0,
            command_palette: CommandPaletteState::new(),
            secret_input: SecretInputState::new(),
            token_sender,
            pairing_tx,
            pairing_code: None,
            discord_connected: false,
            messages_area_height: 0,
            events_area_height: 0,
            active_sessions: HashSet::new(),
            should_quit: false,
            start_time: Instant::now(),
            store,
            config_path,
        }
    }

    pub fn save_secret_and_notify(&mut self) -> Result<()> {
        let token = self.secret_input.input.clone();
        if token.is_empty() {
            self.secret_input.status_message = Some("Token cannot be empty".into());
            return Ok(());
        }

        let key = SecretKey::DiscordBotToken;

        // Store in keyring via injected store
        self.store.set(key.as_str(), &token)?;

        // Mark in config
        let mut config = match &self.config_path {
            Some(p) => ConfigFile::load_from(p)?,
            None => ConfigFile::load()?,
        };
        config.mark_in_keyring(&key);
        match &self.config_path {
            Some(p) => config.save_to(p)?,
            None => config.save()?,
        }

        // Send token via oneshot if available (Setup mode)
        if let Some(sender) = self.token_sender.take() {
            let _ = sender.send(token);
            self.mode = AppMode::Normal;
        } else {
            // Already in Normal mode — just update keyring
            self.push_event("Token updated. Restart to apply.", EventLevel::Info);
        }

        self.secret_input.visible = false;
        self.secret_input.input.clear();
        self.secret_input.status_message = None;
        Ok(())
    }

    pub fn push_event(&mut self, summary: &str, level: EventLevel) {
        self.events.push_back(EventEntry {
            summary: summary.to_string(),
            level,
            timestamp: Instant::now(),
        });
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match &event.kind {
            AppEventKind::DiscordReady => {
                self.discord_connected = true;
            }
            AppEventKind::DiscordDisconnected { .. } => {
                self.discord_connected = false;
            }
            AppEventKind::MessageReceived {
                session_key,
                author,
                content,
            } => {
                self.messages.push_back(MessageEntry {
                    session_key: session_key.clone(),
                    author: author.clone(),
                    content: content.clone(),
                });
                if self.messages.len() > MAX_MESSAGES {
                    self.messages.pop_front();
                }
                self.messages_scroll = 0;
            }
            AppEventKind::ResponseSent {
                session_key,
                content,
            } => {
                self.messages.push_back(MessageEntry {
                    session_key: session_key.clone(),
                    author: "goose".into(),
                    content: content.clone(),
                });
                if self.messages.len() > MAX_MESSAGES {
                    self.messages.pop_front();
                }
                self.messages_scroll = 0;
            }
            AppEventKind::PairingCodeGenerated { code } => {
                self.pairing_code = Some(code.clone());
            }
            AppEventKind::PairingCompleted { session_key } => {
                self.active_sessions.insert(session_key.clone());
            }
            AppEventKind::SessionDisconnected { session_key, .. } => {
                self.active_sessions.remove(session_key);
            }
            AppEventKind::Error { .. } => {}
            AppEventKind::TracingEvent { .. } => {}
        }

        // All events go to the events panel — except message events which
        // are already shown in the messages panel.
        let shown_in_messages = matches!(
            &event.kind,
            AppEventKind::MessageReceived { .. } | AppEventKind::ResponseSent { .. }
        );
        if !shown_in_messages {
            let level = match &event.kind {
                AppEventKind::Error { .. } => EventLevel::Error,
                _ => EventLevel::Info,
            };
            self.events.push_back(EventEntry {
                summary: event.kind.to_string(),
                level,
                timestamp: Instant::now(),
            });
            if self.events.len() > MAX_EVENTS {
                self.events.pop_front();
            }
        }
    }

    /// Count the number of rendered lines in the messages panel.
    /// Must match the rendering logic in ui/messages.rs exactly.
    pub fn messages_line_count(&self) -> usize {
        crate::ui::messages::total_content_height(self)
    }

    pub fn events_line_count(&self) -> usize {
        if self.events.is_empty() {
            1
        } else {
            self.events.len()
        }
    }

    pub fn tick(&mut self) {
        // Periodic housekeeping if needed
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.messages_scroll = 0;
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
        self.events_scroll = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_secrets::{SecretResult, SecretValue};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockStore {
        secrets: Mutex<HashMap<String, String>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self { secrets: Mutex::new(HashMap::new()) }
        }
    }

    impl SecretStore for MockStore {
        fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(self.secrets.lock().unwrap().get(key).map(|v| SecretValue::new(v.clone())))
        }
        fn set(&self, key: &str, value: &str) -> SecretResult<()> {
            self.secrets.lock().unwrap().insert(key.to_owned(), value.to_owned());
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
        assert!(!app.discord_connected);
        app.handle_app_event(make_event(AppEventKind::DiscordReady));
        assert!(app.discord_connected);
    }

    #[test]
    fn test_handle_discord_disconnected() {
        let mut app = test_app();
        app.discord_connected = true;
        app.handle_app_event(make_event(AppEventKind::DiscordDisconnected {
            reason: "test".into(),
        }));
        assert!(!app.discord_connected);
    }

    #[test]
    fn test_handle_message_received() {
        let mut app = test_app();
        app.messages_scroll = 5;
        let sk = SessionKey::dm("user1");
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
        let sk = SessionKey::dm("user1");
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
        let sk = SessionKey::dm("user1");
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
        let sk = SessionKey::dm("user1");
        app.handle_app_event(make_event(AppEventKind::PairingCompleted {
            session_key: sk.clone(),
        }));
        assert!(app.active_sessions.contains(&sk));
    }

    #[test]
    fn test_handle_session_disconnected() {
        let mut app = test_app();
        let sk = SessionKey::dm("user1");
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
            session_key: SessionKey::dm("u"),
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
        let sk = SessionKey::dm("u");
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
        let sk = SessionKey::dm("u");
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
        assert!(!app.discord_connected);
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
        let sk = SessionKey::dm("user1");
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
