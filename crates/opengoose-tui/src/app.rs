use std::collections::VecDeque;
use std::time::Instant;

use anyhow::Result;
use opengoose_secrets::{ConfigFile, KeyringBackend, SecretKey};
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
    pub messages_scroll: u16,
    pub events_scroll: u16,
    pub command_palette: CommandPaletteState,
    pub secret_input: SecretInputState,
    pub token_sender: Option<oneshot::Sender<String>>,
    pub pairing_tx: Option<mpsc::UnboundedSender<()>>,
    pub pairing_code: Option<String>,
    pub discord_connected: bool,
    pub session_count: u32,
    pub messages_area_height: u16,
    pub events_area_height: u16,
    pub should_quit: bool,
    pub start_time: Instant,
}

impl App {
    pub fn new(
        mode: AppMode,
        token_sender: Option<oneshot::Sender<String>>,
        pairing_tx: Option<mpsc::UnboundedSender<()>>,
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
            session_count: 0,
            should_quit: false,
            start_time: Instant::now(),
        }
    }

    pub fn save_secret_and_notify(&mut self) -> Result<()> {
        let token = self.secret_input.input.clone();
        if token.is_empty() {
            self.secret_input.status_message = Some("Token cannot be empty".into());
            return Ok(());
        }

        let key = SecretKey::DiscordBotToken;

        // Store in keyring (blocking, but short)
        KeyringBackend::set(key.as_str(), &token)?;

        // Mark in config
        let mut config = ConfigFile::load()?;
        config.mark_in_keyring(&key);
        config.save()?;

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
            AppEventKind::PairingCompleted { .. } => {
                self.session_count += 1;
            }
            AppEventKind::Error { .. } => {}
            AppEventKind::TracingEvent { .. } => {}
        }

        // All events go to the events panel
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

    /// Count the number of rendered lines in the messages panel.
    /// Must match the rendering logic in ui/messages.rs exactly.
    pub fn messages_line_count(&self) -> u16 {
        crate::ui::messages::total_content_height(self)
    }

    pub fn events_line_count(&self) -> u16 {
        if self.events.is_empty() {
            1
        } else {
            self.events.len() as u16
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
