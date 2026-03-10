use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::state::AppState;

/// JSON response item for a single chat session.
#[derive(Serialize)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for `GET /api/sessions`.
#[derive(Deserialize)]
pub struct ListQuery {
    /// Maximum number of sessions to return (default 50, max 1000).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /api/sessions — list recent chat sessions.
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<SessionItem>>, AppError> {
    if q.limit <= 0 || q.limit > 1000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 1000, got {}",
            q.limit
        )));
    }
    let sessions = state.session_store.list_sessions(q.limit)?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionItem {
                session_key: s.session_key,
                active_team: s.active_team,
                created_at: s.created_at,
                updated_at: s.updated_at,
            })
            .collect(),
    ))
}

/// JSON response item for a single chat message.
#[derive(Serialize)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

/// Query parameters for `GET /api/sessions/{session_key}/messages`.
#[derive(Deserialize)]
pub struct MessagesQuery {
    /// Maximum number of messages to return (default 100, max 5000).
    #[serde(default = "default_msg_limit")]
    pub limit: usize,
}

fn default_msg_limit() -> usize {
    100
}

/// GET /api/sessions/{session_key}/messages — list messages for a session.
pub async fn get_messages(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<MessageItem>>, AppError> {
    if session_key.trim().is_empty() {
        return Err(AppError::BadRequest(
            "`session_key` must not be empty".into(),
        ));
    }
    if q.limit == 0 || q.limit > 5000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 5000, got {}",
            q.limit
        )));
    }
    use opengoose_types::SessionKey;
    // Accept raw stable-id strings directly (e.g. "discord:guild:channel")
    let key = SessionKey::from_stable_id(&session_key);
    let messages = state.session_store.load_history(&key, q.limit)?;
    Ok(Json(
        messages
            .into_iter()
            .map(|m| MessageItem {
                role: m.role,
                content: m.content,
                author: m.author,
                created_at: m.created_at,
            })
            .collect(),
    ))
}
