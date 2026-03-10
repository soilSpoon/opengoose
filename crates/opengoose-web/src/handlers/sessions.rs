use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<SessionItem>>, AppError> {
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

#[derive(Serialize)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct MessagesQuery {
    #[serde(default = "default_msg_limit")]
    pub limit: usize,
}

fn default_msg_limit() -> usize {
    100
}

pub async fn get_messages(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<MessageItem>>, AppError> {
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
