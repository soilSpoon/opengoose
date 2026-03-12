use axum::Json;
use axum::extract::{Path, Query, State};
use opengoose_types::SessionKey;
use serde::Deserialize;

use crate::handlers::AppError;
use crate::state::AppState;

use super::MessageItem;

/// Query parameters for `GET /api/sessions/{session_key}/messages`.
#[derive(Deserialize)]
pub struct MessagesQuery {
    /// Maximum number of messages to return (default 100, max 5000).
    #[serde(default = "default_msg_limit")]
    pub limit: usize,
}

pub(super) fn default_msg_limit() -> usize {
    100
}

impl MessagesQuery {
    fn validated_limit(&self) -> Result<usize, AppError> {
        if self.limit == 0 || self.limit > 5000 {
            return Err(AppError::UnprocessableEntity(format!(
                "`limit` must be between 1 and 5000, got {}",
                self.limit
            )));
        }

        Ok(self.limit)
    }
}

fn parse_session_key(session_key: &str) -> Result<SessionKey, AppError> {
    if session_key.trim().is_empty() {
        return Err(AppError::BadRequest(
            "`session_key` must not be empty".into(),
        ));
    }

    // Accept raw stable-id strings directly (e.g. "discord:guild:channel")
    Ok(SessionKey::from_stable_id(session_key))
}

/// GET /api/sessions/{session_key}/messages — list messages for a session.
pub async fn get_messages(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<MessageItem>>, AppError> {
    let key = parse_session_key(&session_key)?;
    let messages = state
        .session_store
        .load_history(&key, q.validated_limit()?)?;
    Ok(Json(messages.into_iter().map(MessageItem::from).collect()))
}
