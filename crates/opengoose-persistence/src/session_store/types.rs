use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Integer, Nullable, Text};
use serde::Serialize;

/// A session row returned by list queries.
#[derive(Debug, Clone)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Alias for backward compatibility with code using SessionSummary.
pub type SessionSummary = SessionItem;

/// Aggregate session statistics.
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub session_count: i64,
    pub message_count: i64,
    pub estimated_token_count: i64,
    pub active_session_count: i64,
    pub average_session_duration_seconds: f64,
}

/// Session metric details for a single stored session.
#[derive(Debug, Clone)]
pub struct SessionMetricItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub estimated_token_count: i64,
    pub duration_seconds: i64,
    pub active: bool,
}

/// A conversation message stored in the database.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

/// Export payload for a single stored session and its full message history.
#[derive(Debug, Clone, Serialize)]
pub struct SessionExport {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub messages: Vec<HistoryMessage>,
}

/// Query filters for batch session export.
#[derive(Debug, Clone)]
pub struct SessionExportQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: i64,
}

impl Default for SessionExportQuery {
    fn default() -> Self {
        Self {
            since: None,
            until: None,
            limit: 100,
        }
    }
}

#[derive(QueryableByName)]
pub(crate) struct SessionStatsRow {
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
    #[diesel(sql_type = BigInt)]
    pub message_count: i64,
    #[diesel(sql_type = BigInt)]
    pub estimated_token_count: i64,
    #[diesel(sql_type = BigInt)]
    pub active_session_count: i64,
    #[diesel(sql_type = Double)]
    pub average_session_duration_seconds: f64,
}

#[derive(QueryableByName)]
pub(crate) struct SessionMetricRow {
    #[diesel(sql_type = Text)]
    pub session_key: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub active_team: Option<String>,
    #[diesel(sql_type = Text)]
    pub created_at: String,
    #[diesel(sql_type = Text)]
    pub updated_at: String,
    #[diesel(sql_type = BigInt)]
    pub message_count: i64,
    #[diesel(sql_type = BigInt)]
    pub estimated_token_count: i64,
    #[diesel(sql_type = BigInt)]
    pub duration_seconds: i64,
    #[diesel(sql_type = Integer)]
    pub active: i32,
}
