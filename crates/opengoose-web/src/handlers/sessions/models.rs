use opengoose_persistence::{HistoryMessage, SessionItem as StoredSessionItem};
use serde::Serialize;

/// JSON response item for a single chat session.
#[derive(Serialize)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub selected_model: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<StoredSessionItem> for SessionItem {
    fn from(value: StoredSessionItem) -> Self {
        Self {
            session_key: value.session_key,
            active_team: value.active_team,
            selected_model: value.selected_model,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// JSON response item for a single chat message.
#[derive(Serialize)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

impl From<HistoryMessage> for MessageItem {
    fn from(value: HistoryMessage) -> Self {
        Self {
            role: value.role,
            content: value.content,
            author: value.author,
            created_at: value.created_at,
        }
    }
}
