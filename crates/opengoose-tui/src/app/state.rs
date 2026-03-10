use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use opengoose_persistence::{Database, SessionStore};
use opengoose_provider_bridge::ProviderSummary;
use opengoose_secrets::{SecretStore, default_store};
use opengoose_types::{Platform, SessionKey};
use tokio::sync::{mpsc, oneshot};

use crate::ComposerRequest;

pub(crate) const MAX_MESSAGES: usize = 1000;
pub(crate) const MAX_EVENTS: usize = 2000;
const SESSION_HISTORY_LIMIT: usize = 200;
const SESSION_LIST_LIMIT: i64 = 100;
const COMPOSER_HISTORY_LIMIT: usize = 50;
const LOCAL_COMPOSER_PLATFORM: &str = "tui";
const LOCAL_COMPOSER_SESSION_ID: &str = "local";

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
    Sessions,
    Messages,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Generating,
}

#[derive(Debug, Clone)]
pub struct SessionListEntry {
    pub session_key: SessionKey,
    pub active_team: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct StatusNotice {
    pub message: String,
    pub level: EventLevel,
}

pub struct ComposerState {
    pub input: String,
    pub cursor: usize,
    pub history: VecDeque<String>,
    pub history_index: Option<usize>,
    pub history_draft: Option<String>,
}

impl ComposerState {
    pub(crate) fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            history: VecDeque::new(),
            history_index: None,
            history_draft: None,
        }
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
        self.history_draft = None;
    }

    pub fn insert_char(&mut self, c: char) {
        self.history_index = None;
        self.history_draft = None;
        let index = byte_index_for_char(&self.input, self.cursor);
        self.input.insert(index, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.history_index = None;
        self.history_draft = None;
        let end = byte_index_for_char(&self.input, self.cursor);
        let start = byte_index_for_char(&self.input, self.cursor - 1);
        self.input.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.input.chars().count() {
            return;
        }
        self.history_index = None;
        self.history_draft = None;
        let start = byte_index_for_char(&self.input, self.cursor);
        let end = byte_index_for_char(&self.input, self.cursor + 1);
        self.input.replace_range(start..end, "");
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.input.chars().count();
    }

    pub fn push_history(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        self.history.retain(|existing| existing != &entry);
        self.history.push_back(entry);
        while self.history.len() > COMPOSER_HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.history_index = None;
        self.history_draft = None;
    }

    pub fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            Some(0) => {}
            Some(index) => {
                self.history_index = Some(index - 1);
            }
            None => {
                self.history_draft = Some(self.input.clone());
                self.history_index = Some(self.history.len() - 1);
            }
        }

        if let Some(index) = self.history_index {
            self.input = self.history[index].clone();
            self.cursor = self.input.chars().count();
        }
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };

        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.input = self.history[index + 1].clone();
        } else {
            self.history_index = None;
            self.input = self.history_draft.take().unwrap_or_default();
        }

        self.cursor = self.input.chars().count();
    }
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

    pub fn set_composer_tx(&mut self, composer_tx: mpsc::UnboundedSender<ComposerRequest>) {
        self.composer_tx = Some(composer_tx);
    }

    pub fn composer_session_key(&self) -> SessionKey {
        self.selected_session
            .clone()
            .unwrap_or_else(Self::default_composer_session_key)
    }

    pub fn attach_session_store(&mut self, session_store: Arc<SessionStore>) {
        self.session_store = Some(session_store.clone());

        if let Ok(active_teams) = session_store.load_all_active_teams() {
            self.active_teams.extend(active_teams);
        }

        self.refresh_sessions();
    }

    pub fn refresh_sessions(&mut self) {
        let mut session_map: HashMap<String, SessionListEntry> = self
            .sessions
            .iter()
            .cloned()
            .map(|entry| (entry.session_key.to_stable_id(), entry))
            .collect();

        if let Some(store) = &self.session_store {
            match store.list_sessions(SESSION_LIST_LIMIT) {
                Ok(items) => {
                    for item in items {
                        let session_key = SessionKey::from_stable_id(&item.session_key);
                        session_map.insert(
                            item.session_key,
                            SessionListEntry {
                                active_team: item
                                    .active_team
                                    .or_else(|| self.active_teams.get(&session_key).cloned()),
                                created_at: Some(item.created_at),
                                updated_at: Some(item.updated_at),
                                is_active: self.active_sessions.contains(&session_key),
                                session_key,
                            },
                        );
                    }
                }
                Err(e) => {
                    let message = format!("Could not load session history: {e}");
                    self.push_event(&message, EventLevel::Error);
                    self.set_status_notice(message, EventLevel::Error);
                }
            }
        }

        for session_key in &self.active_sessions {
            let stable = session_key.to_stable_id();
            session_map
                .entry(stable)
                .and_modify(|entry| {
                    entry.is_active = true;
                    if entry.active_team.is_none() {
                        entry.active_team = self.active_teams.get(session_key).cloned();
                    }
                })
                .or_insert_with(|| SessionListEntry {
                    session_key: session_key.clone(),
                    active_team: self.active_teams.get(session_key).cloned(),
                    created_at: None,
                    updated_at: None,
                    is_active: true,
                });
        }

        let previous_selection = self.selected_session.clone();
        let mut sessions = session_map.into_values().collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            right
                .is_active
                .cmp(&left.is_active)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| {
                    left.session_key
                        .to_stable_id()
                        .cmp(&right.session_key.to_stable_id())
                })
        });
        self.sessions = sessions;

        if self.sessions.is_empty() {
            self.selected_session = None;
            self.selected_session_index = 0;
            self.messages.clear();
            self.messages_scroll = 0;
            return;
        }

        if let Some(session_key) = previous_selection
            && let Some(index) = self
                .sessions
                .iter()
                .position(|entry| entry.session_key == session_key)
        {
            self.selected_session_index = index;
            self.selected_session = Some(self.sessions[index].session_key.clone());
            self.ensure_selected_session_visible();
            if self.messages.is_empty() {
                self.load_selected_session_history();
            }
            return;
        }

        self.select_session(self.selected_session_index.min(self.sessions.len() - 1));
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

    pub fn clear_messages(&mut self) {
        if let Some(session_key) = &self.selected_session {
            self.session_messages.remove(session_key);
        } else {
            self.session_messages.clear();
        }
        self.messages.clear();
        self.messages_scroll = 0;
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

    pub fn submit_composer(&mut self) {
        let content = self.composer.input.clone();
        if content.trim().is_empty() {
            return;
        }
        let session_key = self.composer_session_key();

        let Some(tx) = &self.composer_tx else {
            let message = "Message sending is unavailable in the current TUI mode.".to_string();
            self.push_event(&message, EventLevel::Error);
            self.set_status_notice(message, EventLevel::Error);
            return;
        };

        if tx
            .send(ComposerRequest {
                session_key,
                content: content.clone(),
            })
            .is_err()
        {
            let message = "Failed to submit the message to the local engine.".to_string();
            self.push_event(&message, EventLevel::Error);
            self.set_status_notice(message, EventLevel::Error);
            return;
        }

        self.composer.push_history(content);
        self.composer.clear();
    }

    pub fn focus_sessions(&mut self) {
        self.active_panel = Panel::Sessions;
        if self.selected_session.is_none() && !self.sessions.is_empty() {
            self.select_session(0);
        }
    }

    pub fn select_next_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let next = (self.selected_session_index + 1).min(self.sessions.len() - 1);
        self.select_session(next);
    }

    pub fn select_previous_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.select_session(self.selected_session_index.saturating_sub(1));
    }

    pub fn select_first_session(&mut self) {
        if !self.sessions.is_empty() {
            self.select_session(0);
        }
    }

    pub fn select_last_session(&mut self) {
        if !self.sessions.is_empty() {
            self.select_session(self.sessions.len() - 1);
        }
    }

    pub fn select_session(&mut self, index: usize) {
        if self.sessions.is_empty() {
            return;
        }

        self.selected_session_index = index.min(self.sessions.len() - 1);
        self.selected_session = Some(
            self.sessions[self.selected_session_index]
                .session_key
                .clone(),
        );
        self.ensure_selected_session_visible();
        self.load_selected_session_history();
        self.messages_scroll = 0;
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

    pub fn cache_message(&mut self, entry: MessageEntry) {
        let session_key = entry.session_key.clone();
        if let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.session_key == session_key)
        {
            let mut session = self.sessions.remove(index);
            session.is_active = self.active_sessions.contains(&session_key) || session.is_active;
            session.active_team = self
                .active_teams
                .get(&session_key)
                .cloned()
                .or(session.active_team);
            self.sessions.insert(0, session);
            if self.selected_session.as_ref() == Some(&session_key) {
                self.selected_session_index = 0;
            }
        } else {
            self.sessions.insert(
                0,
                SessionListEntry {
                    session_key: session_key.clone(),
                    active_team: self.active_teams.get(&session_key).cloned(),
                    created_at: None,
                    updated_at: None,
                    is_active: self.active_sessions.contains(&session_key),
                },
            );
        }

        let messages = self
            .session_messages
            .entry(session_key.clone())
            .or_default();
        messages.push_back(entry);
        if messages.len() > MAX_MESSAGES {
            messages.pop_front();
        }

        if self.selected_session.as_ref() == Some(&session_key) {
            self.messages = messages.clone();
            self.messages_scroll = 0;
        } else if self.selected_session.is_none() {
            self.selected_session_index = 0;
            self.selected_session = Some(session_key);
            self.load_selected_session_history();
            self.messages_scroll = 0;
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

    fn ensure_selected_session_visible(&mut self) {
        if self.sessions_area_height == 0 {
            return;
        }

        if self.selected_session_index < self.sessions_scroll {
            self.sessions_scroll = self.selected_session_index;
            return;
        }

        let last_visible = self
            .sessions_scroll
            .saturating_add(self.sessions_area_height.saturating_sub(1));
        if self.selected_session_index > last_visible {
            self.sessions_scroll = self
                .selected_session_index
                .saturating_add(1)
                .saturating_sub(self.sessions_area_height);
        }
    }

    fn load_selected_session_history(&mut self) {
        let Some(session_key) = self.selected_session.clone() else {
            self.messages.clear();
            return;
        };

        if !self.session_messages.contains_key(&session_key)
            && let Some(store) = &self.session_store
        {
            match store.load_history(&session_key, SESSION_HISTORY_LIMIT) {
                Ok(history) => {
                    let messages = history
                        .into_iter()
                        .map(|message| MessageEntry {
                            session_key: session_key.clone(),
                            author: message
                                .author
                                .unwrap_or_else(|| match message.role.as_str() {
                                    "assistant" => "goose".to_string(),
                                    _ => "user".to_string(),
                                }),
                            content: message.content,
                        })
                        .collect::<VecDeque<_>>();
                    self.session_messages.insert(session_key.clone(), messages);
                }
                Err(e) => {
                    let message = format!(
                        "Could not load history for {}: {e}",
                        Self::format_session_label(&session_key)
                    );
                    self.push_event(&message, EventLevel::Error);
                    self.set_status_notice(message, EventLevel::Error);
                }
            }
        }

        self.messages = self
            .session_messages
            .get(&session_key)
            .cloned()
            .unwrap_or_default();
    }

    fn default_composer_session_key() -> SessionKey {
        SessionKey::direct(
            Platform::Custom(LOCAL_COMPOSER_PLATFORM.to_string()),
            LOCAL_COMPOSER_SESSION_ID,
        )
    }
}

fn byte_index_for_char(input: &str, char_idx: usize) -> usize {
    input
        .char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}

#[cfg(test)]
mod tests {
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
}
