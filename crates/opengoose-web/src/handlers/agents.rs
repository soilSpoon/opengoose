use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

/// JSON response item representing an agent profile name.
#[derive(Serialize)]
pub struct AgentItem {
    /// Profile name (e.g. "developer", "researcher").
    pub name: String,
}

/// GET /api/agents — list all installed agent profiles.
pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<AgentItem>>, AppError> {
    let names = state.profile_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| AgentItem { name }).collect(),
    ))
}
