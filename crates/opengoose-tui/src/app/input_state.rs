use std::collections::VecDeque;

use opengoose_types::{Platform, SessionKey};
use tokio::sync::mpsc;

use super::session_state::EventLevel;
use super::state::App;
use crate::ComposerRequest;

const COMPOSER_HISTORY_LIMIT: usize = 50;
const LOCAL_COMPOSER_PLATFORM: &str = "tui";
const LOCAL_COMPOSER_SESSION_ID: &str = "local";

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

impl App {
    pub fn set_composer_tx(&mut self, composer_tx: mpsc::UnboundedSender<ComposerRequest>) {
        self.composer_tx = Some(composer_tx);
    }

    pub fn composer_session_key(&self) -> SessionKey {
        self.selected_session
            .clone()
            .unwrap_or_else(Self::default_composer_session_key)
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
