use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct TeamItem {
    pub name: String,
}

pub async fn list_teams(State(state): State<AppState>) -> Result<Json<Vec<TeamItem>>, AppError> {
    let names = state.team_store.list()?;
    Ok(Json(names.into_iter().map(|name| TeamItem { name }).collect()))
}
