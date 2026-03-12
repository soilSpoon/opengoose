use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use opengoose_persistence::{Database, SessionStore};
use opengoose_provider_bridge::ProviderSummary;
use opengoose_secrets::{default_store, SecretStore};
use opengoose_types::{Platform, SessionKey};
use tokio::sync::{mpsc, oneshot};

use crate::ComposerRequest;

pub use super::input_state::{
    CommandPaletteState, ComposerState, CredentialFlowState, CredentialKey, ModelSelectState,
    ProviderSelectPurpose, ProviderSelectState, SecretInputState,
};
pub use super::session_types::MAX_EVENTS;
pub use super::session_types::{
    AgentStatus, EventEntry, EventLevel, MessageEntry, Panel, SessionListEntry, StatusNotice,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Setup,
    Normal,
}

pub struct App {
    pub mode: AppMode,
    pub messages: VecDeque<MessageEntry>,
    pub events: VecDeque<EventEntry>,
    pub sessions: Vec<SessionListEntry>,
    pub active_panel: Panel,
    pub composer: ComposerState,
    pub messages_scroll: usize,
    pub events_scroll: usize,
    pub sessions_scroll: usize,
    pub selected_session: Option<SessionKey>,
    pub selected_session_index: usize,
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
    pub messages_area_width: usize,
    pub events_area_height: usize,
    pub sessions_area_height: usize,
    pub should_quit: bool,
    pub start_time: Instant,
    pub agent_status: AgentStatus,
    pub agent_status_session: Option<SessionKey>,
    pub status_notice: Option<StatusNotice>,
    pub active_teams: HashMap<SessionKey, String>,
    pub cached_providers: Vec<ProviderSummary>,
    pub provider_loading_rx: Option<oneshot::Receiver<Vec<ProviderSummary>>>,
    pub model_loading_rx: Option<oneshot::Receiver<Vec<String>>>,
    pub oauth_done_rx: Option<oneshot::Receiver<anyhow::Result<()>>>,
    pub(crate) composer_tx: Option<mpsc::UnboundedSender<ComposerRequest>>,
    pub(crate) store: Arc<dyn SecretStore>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) session_store: Option<Arc<SessionStore>>,
    pub(crate) session_messages: HashMap<SessionKey, VecDeque<MessageEntry>>,
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
            sessions: Vec::new(),
            active_panel: Panel::Messages,
            composer: ComposerState::new(),
            messages_scroll: 0,
            events_scroll: 0,
            sessions_scroll: 0,
            selected_session: None,
            selected_session_index: 0,
            command_palette: CommandPaletteState::new(),
            secret_input: SecretInputState::new(),
            provider_select: ProviderSelectState::new(),
            credential_flow: CredentialFlowState::new(),
            model_select: ModelSelectState::new(),
            token_sender,
            pairing_tx,
            pairing_code: None,
            connected_platforms: HashSet::new(),
            active_sessions: HashSet::new(),
            messages_area_height: 0,
            messages_area_width: 0,
            events_area_height: 0,
            sessions_area_height: 0,
            should_quit: false,
            start_time: Instant::now(),
            agent_status: AgentStatus::Idle,
            agent_status_session: None,
            status_notice: None,
            active_teams: HashMap::new(),
            cached_providers: Vec::new(),
            provider_loading_rx: None,
            model_loading_rx: None,
            oauth_done_rx: None,
            composer_tx: None,
            store,
            config_path,
            session_store: None,
            session_messages: HashMap::new(),
        }
    }

    pub fn initialize_runtime_state(&mut self) {
        match Database::open() {
            Ok(db) => self.attach_session_store(Arc::new(SessionStore::new(Arc::new(db)))),
            Err(e) => {
                let message = format!("Session history is unavailable: {e}");
                self.push_event(&message, EventLevel::Error);
                self.set_status_notice(message, EventLevel::Error);
            }
        }
    }

    pub fn request_new_session(&mut self) {
        match &self.pairing_tx {
            Some(tx) => {
                let _ = tx.send(());
                self.push_event(
                    "Requested a new pairing code for the next session.",
                    EventLevel::Info,
                );
            }
            None => {
                let message = "New sessions are not available in this mode.".to_string();
                self.push_event(&message, EventLevel::Error);
                self.set_status_notice(message, EventLevel::Error);
            }
        }
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
        self.events_scroll = 0;
    }

    pub fn set_status_notice(&mut self, message: String, level: EventLevel) {
        self.status_notice = Some(StatusNotice { message, level });
    }

    pub fn set_agent_status(&mut self, status: AgentStatus, session_key: Option<SessionKey>) {
        self.agent_status = status;
        self.agent_status_session = session_key;
    }

    pub fn format_session_label(session_key: &SessionKey) -> String {
        match &session_key.namespace {
            Some(namespace) => format!(
                "{}:{}/{}",
                session_key.platform.as_str(),
                namespace,
                session_key.channel_id
            ),
            None => format!(
                "{}:{}",
                session_key.platform.as_str(),
                session_key.channel_id
            ),
        }
    }

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
