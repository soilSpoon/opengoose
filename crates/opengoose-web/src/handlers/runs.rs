use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct RunItem {
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<RunItem>>, AppError> {
    use opengoose_persistence::RunStatus;
    let status = q.status.as_deref().and_then(|s| RunStatus::parse(s).ok());
    let runs = state
        .orchestration_store
        .list_runs(status.as_ref(), q.limit)?;
    Ok(Json(
        runs.into_iter()
            .map(|r| RunItem {
                team_run_id: r.team_run_id,
                session_key: r.session_key,
                team_name: r.team_name,
                workflow: r.workflow,
                status: r.status.as_str().to_string(),
                current_step: r.current_step,
                total_steps: r.total_steps,
                result: r.result,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}
