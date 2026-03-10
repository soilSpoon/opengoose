use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AgentItem {
    pub name: String,
}

pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<AgentItem>>, AppError> {
    let names = state.profile_store.list()?;
    Ok(Json(
        names.into_iter().map(|name| AgentItem { name }).collect(),
    ))
}
