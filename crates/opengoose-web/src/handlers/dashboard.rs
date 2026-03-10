use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct DashboardStats {
    pub session_count: i64,
    pub message_count: i64,
    pub run_count: i64,
    pub agent_count: usize,
    pub team_count: usize,
}

pub async fn get_dashboard(
    State(state): State<AppState>,
) -> Result<Json<DashboardStats>, AppError> {
    let session_stats = state.session_store.stats()?;
    let runs = state.orchestration_store.list_runs(None, i64::MAX)?;
    let agent_count = state.profile_store.list().map(|v| v.len()).unwrap_or(0);
    let team_count = state.team_store.list().map(|v| v.len()).unwrap_or(0);

    Ok(Json(DashboardStats {
        session_count: session_stats.session_count,
        message_count: session_stats.message_count,
        run_count: runs.len() as i64,
        agent_count,
        team_count,
    }))
}
