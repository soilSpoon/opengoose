use std::sync::Arc;

use tracing::warn;

use opengoose_persistence::{Database, SessionStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey};

/// Handles message persistence (user and assistant messages).
///
/// Extracted from Engine so that message-recording concerns are
/// isolated from team/session management and orchestration logic.
pub struct MessageRecorder {
    db: Arc<Database>,
    event_bus: EventBus,
}

impl MessageRecorder {
    pub fn new(db: Arc<Database>, event_bus: EventBus) -> Self {
        Self { db, event_bus }
    }

    pub fn record_user_message(&self, key: &SessionKey, content: &str, author: Option<&str>) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_user_message(key, content, author) {
            warn!(%e, "failed to persist user message");
        }
    }

    pub fn record_assistant_message(&self, key: &SessionKey, content: &str) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_assistant_message(key, content) {
            warn!(%e, "failed to persist assistant message");
        }
    }

    /// Record an assistant message and emit a ResponseSent event.
    pub fn send_response(&self, session_key: &SessionKey, msg: &str) {
        self.record_assistant_message(session_key, msg);
        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: msg.to_string(),
        });
    }
}
