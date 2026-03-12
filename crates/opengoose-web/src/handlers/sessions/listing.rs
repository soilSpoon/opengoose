use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::handlers::AppError;
use crate::state::AppState;

use super::SessionItem;

/// Query parameters for `GET /api/sessions`.
#[derive(Deserialize)]
pub struct ListQuery {
    /// Maximum number of sessions to return (default 50, max 1000).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

pub(super) fn default_limit() -> i64 {
    50
}

impl ListQuery {
    fn validated_limit(&self) -> Result<i64, AppError> {
        if self.limit <= 0 || self.limit > 1000 {
            return Err(AppError::UnprocessableEntity(format!(
                "`limit` must be between 1 and 1000, got {}",
                self.limit
            )));
        }

        Ok(self.limit)
    }
}

/// GET /api/sessions — list recent chat sessions.
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<SessionItem>>, AppError> {
    let sessions = state.session_store.list_sessions(q.validated_limit()?)?;
    Ok(Json(sessions.into_iter().map(SessionItem::from).collect()))
}
