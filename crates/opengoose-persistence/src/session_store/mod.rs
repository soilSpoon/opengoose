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

use std::collections::HashMap;
use std::sync::Arc;

use opengoose_types::SessionKey;

use crate::db::Database;
use crate::error::PersistenceResult;

pub use export::{render_batch_session_exports_markdown, render_session_export_markdown};
pub use types::{
    HistoryMessage, SessionExport, SessionExportQuery, SessionItem, SessionMetricItem,
    SessionStats, SessionSummary,
};

/// Session and conversation history operations on a shared Database.
pub struct SessionStore {
    db: Arc<Database>,
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
        mutations::SessionMutations::append_message(&self.db, key, "user", content, author)
    }

    /// Append an assistant message to the conversation history.
    pub fn append_assistant_message(
        &self,
        key: &SessionKey,
        content: &str,
    ) -> PersistenceResult<()> {
        mutations::SessionMutations::append_message(
            &self.db,
            key,
            "assistant",
            content,
            Some("goose"),
        )
    }

    /// Load the most recent messages for a session.
    pub fn load_history(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        queries::SessionQueries::load_history(&self.db, key, limit)
    }

    /// Set or clear the active team for a session.
    pub fn set_active_team(&self, key: &SessionKey, team: Option<&str>) -> PersistenceResult<()> {
        mutations::SessionMutations::set_active_team(&self.db, key, team)
    }

    /// Get the active team for a session.
    pub fn get_active_team(&self, key: &SessionKey) -> PersistenceResult<Option<String>> {
        queries::SessionQueries::get_active_team(&self.db, key)
    }

    /// Load all sessions that have an active team set.
    pub fn load_all_active_teams(&self) -> PersistenceResult<HashMap<SessionKey, String>> {
        queries::SessionQueries::load_all_active_teams(&self.db)
    }

    /// List sessions ordered by most recently updated, limited to `limit` results.
    pub fn list_sessions(&self, limit: i64) -> PersistenceResult<Vec<SessionItem>> {
        queries::SessionQueries::list_sessions(&self.db, limit)
    }

    /// Load a single session export including all persisted messages.
    pub fn export_session(&self, key: &SessionKey) -> PersistenceResult<Option<SessionExport>> {
        queries::SessionQueries::export_session(&self.db, key)
    }

    /// Load multiple session exports filtered by session activity window.
    pub fn export_sessions(
        &self,
        query: &SessionExportQuery,
    ) -> PersistenceResult<Vec<SessionExport>> {
        queries::SessionQueries::export_sessions(&self.db, query)
    }

    /// Return aggregate statistics (session count and message count).
    pub fn stats(&self) -> PersistenceResult<SessionStats> {
        queries::SessionQueries::stats(&self.db)
    }

    /// List per-session metrics ordered by most recently updated session first.
    ///
    /// Token usage is estimated using a coarse `~4 chars/token` heuristic because
    /// persisted message rows do not currently store model-native token counts.
    pub fn list_session_metrics(&self, limit: i64) -> PersistenceResult<Vec<SessionMetricItem>> {
        queries::SessionQueries::list_session_metrics(&self.db, limit)
    }

    /// Delete individual messages older than the given retention window.
    pub fn cleanup_expired_messages(&self, retention_days: u32) -> PersistenceResult<usize> {
        mutations::SessionMutations::cleanup_expired_messages(&self.db, retention_days)
    }

    /// Delete sessions and messages older than the given number of hours.
    pub fn cleanup(&self, max_age_hours: i64) -> PersistenceResult<usize> {
        mutations::SessionMutations::cleanup(&self.db, max_age_hours)
    }
}
