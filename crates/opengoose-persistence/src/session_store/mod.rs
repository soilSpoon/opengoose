//! Session and conversation history persistence.
//!
//! Provides [`SessionStore`] for managing session lifecycle, message history,
//! session metrics, and conversation export. Markdown rendering for session
//! exports lives in the [`export`] submodule.

pub mod export;
mod mutations;
mod queries;
mod tests;
pub mod types;

use std::sync::Arc;

use opengoose_types::SessionKey;

use crate::db::Database;
use crate::error::PersistenceResult;

pub use export::{render_batch_session_exports_markdown, render_session_export_markdown};
pub use types::{HistoryMessage, SessionExport, SessionExportQuery, SessionItem, SessionMetricItem, SessionStats};

/// Session and conversation history operations on a shared Database.
pub struct SessionStore {
    pub(crate) db: Arc<Database>,
}

impl SessionStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Append a user message to the conversation history.
    pub fn append_user_message(
        &self,
        key: &SessionKey,
        content: &str,
        author: Option<&str>,
    ) -> PersistenceResult<()> {
        self.append_message(key, "user", content, author)
    }

    /// Append an assistant message to the conversation history.
    pub fn append_assistant_message(
        &self,
        key: &SessionKey,
        content: &str,
    ) -> PersistenceResult<()> {
        self.append_message(key, "assistant", content, Some("goose"))
    }
}
