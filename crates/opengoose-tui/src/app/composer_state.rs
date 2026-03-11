use opengoose_types::{Platform, SessionKey};
use tokio::sync::mpsc;

use super::session_types::EventLevel;
use super::state::App;
use crate::ComposerRequest;

const LOCAL_COMPOSER_PLATFORM: &str = "tui";
const LOCAL_COMPOSER_SESSION_ID: &str = "local";

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

        let Some(tx) = &self.composer_tx else {
            let message = "Message sending is unavailable in the current TUI mode.".to_string();
            self.push_event(&message, EventLevel::Error);
            self.set_status_notice(message, EventLevel::Error);
            return;
        };

        if tx
            .send(ComposerRequest {
                session_key: self.composer_session_key(),
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
