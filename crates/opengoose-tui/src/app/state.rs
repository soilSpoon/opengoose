use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use opengoose_provider_bridge::ProviderSummary;
use opengoose_secrets::{SecretStore, default_store};
use opengoose_types::{Platform, SessionKey};
use tokio::sync::{mpsc, oneshot};

pub(crate) const MAX_MESSAGES: usize = 1000;
pub(crate) const MAX_EVENTS: usize = 2000;

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
    /// Custom title for the input dialog. `None` uses "Discord Bot Token".
    pub title: Option<String>,
    /// Whether to mask input. Defaults to `true`.
    pub is_secret: bool,
}

impl SecretInputState {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            status_message: None,
            title: None,
            is_secret: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSelectPurpose {
    Configure,
    ListModels,
}

pub struct CommandPaletteState {
    pub visible: bool,
    pub input: String,
    pub selected: usize,
}

impl CommandPaletteState {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            selected: 0,
        }
    }
}

pub struct ProviderSelectState {
    pub visible: bool,
    pub providers: Vec<String>,
    pub provider_ids: Vec<String>,
    pub selected: usize,
    pub purpose: ProviderSelectPurpose,
}

impl ProviderSelectState {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            providers: Vec::new(),
            provider_ids: Vec::new(),
            selected: 0,
            purpose: ProviderSelectPurpose::Configure,
        }
    }
}

#[derive(Clone)]
pub struct CredentialKey {
    pub env_var: String,
    pub label: String,
    pub secret: bool,
    pub oauth_flow: bool,
    pub required: bool,
    pub default: Option<String>,
}

pub struct CredentialFlowState {
    pub provider_id: Option<String>,
    pub provider_display: Option<String>,
    pub keys: Vec<CredentialKey>,
    pub current_key: usize,
    pub collected: Vec<(String, String)>,
}

impl CredentialFlowState {
    pub(crate) fn new() -> Self {
        Self {
            provider_id: None,
            provider_display: None,
            keys: Vec::new(),
            current_key: 0,
            collected: Vec::new(),
        }
    }

    pub fn current(&self) -> Option<&CredentialKey> {
        self.keys.get(self.current_key)
    }

    pub fn has_more(&self) -> bool {
        self.current_key + 1 < self.keys.len()
    }

    pub fn reset(&mut self) {
        self.provider_id = None;
        self.provider_display = None;
        self.keys.clear();
        self.current_key = 0;
        self.collected.clear();
    }
}

pub struct ModelSelectState {
    pub visible: bool,
    pub models: Vec<String>,
    pub selected: usize,
    pub loading: bool,
    pub provider_name: String,
}

impl ModelSelectState {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            models: Vec::new(),
            selected: 0,
            loading: false,
            provider_name: String::new(),
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
    pub provider_select: ProviderSelectState,
    pub credential_flow: CredentialFlowState,
    pub model_select: ModelSelectState,
    pub token_sender: Option<oneshot::Sender<String>>,
    pub pairing_tx: Option<mpsc::UnboundedSender<()>>,
    pub pairing_code: Option<String>,
    pub connected_platforms: HashSet<Platform>,
    pub active_sessions: HashSet<SessionKey>,
    pub messages_area_height: usize,
    pub events_area_height: usize,
    pub should_quit: bool,
    pub start_time: Instant,
    /// Per-channel active teams (mirrored from gateway events)
    pub active_teams: HashMap<SessionKey, String>,
    /// Cached provider summaries from Goose.
    pub cached_providers: Vec<ProviderSummary>,
    /// Receiver for async provider list loading.
    pub provider_loading_rx: Option<oneshot::Receiver<Vec<ProviderSummary>>>,
    /// Receiver for async model list loading.
    pub model_loading_rx: Option<oneshot::Receiver<Vec<String>>>,
    /// Receiver for async OAuth completion.
    pub oauth_done_rx: Option<oneshot::Receiver<anyhow::Result<()>>>,
    pub(crate) store: Arc<dyn SecretStore>,
    pub(crate) config_path: Option<PathBuf>,
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
            provider_select: ProviderSelectState::new(),
            credential_flow: CredentialFlowState::new(),
            model_select: ModelSelectState::new(),
            token_sender,
            pairing_tx,
            pairing_code: None,
            connected_platforms: HashSet::new(),
            messages_area_height: 0,
            events_area_height: 0,
            active_sessions: HashSet::new(),
            should_quit: false,
            start_time: Instant::now(),
            active_teams: HashMap::new(),
            cached_providers: Vec::new(),
            provider_loading_rx: None,
            model_loading_rx: None,
            oauth_done_rx: None,
            store,
            config_path,
        }
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.messages_scroll = 0;
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
        self.events_scroll = 0;
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
}
