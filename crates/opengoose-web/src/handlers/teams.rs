use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

/// JSON response item representing a team definition name.
#[derive(Serialize)]
pub struct TeamItem {
    /// Team name (e.g. "code-review", "feature-dev").
    pub name: String,
}

/// GET /api/teams — list all installed team definitions.
pub async fn list_teams(State(state): State<AppState>) -> Result<Json<Vec<TeamItem>>, AppError> {
    let names = state.team_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| TeamItem { name }).collect(),
    ))
}
