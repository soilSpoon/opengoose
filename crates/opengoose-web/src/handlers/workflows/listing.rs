use axum::Json;
use axum::extract::{Path, State};

use crate::handlers::AppError;
use crate::state::AppState;

use super::history::{build_workflow_detail, build_workflow_item};
use super::{WorkflowDetail, WorkflowItem};

/// GET /api/workflows — list all workflow definitions with automation summary.
pub async fn list_workflows(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkflowItem>>, AppError> {
    let names = state.team_store.list()?;
    let runs = state.orchestration_store.list_runs(None, 200)?;
    let schedules = state.schedule_store.list()?;
    let triggers = state.trigger_store.list()?;

    let workflows = names
        .into_iter()
        .map(|name| {
            let team = state.team_store.get(&name)?;
            Ok(build_workflow_item(
                &name, &team, &schedules, &triggers, &runs,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(Json(workflows))
}

/// GET /api/workflows/:name — return a single workflow definition plus automation and run history.
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<WorkflowDetail>, AppError> {
    let team = state.team_store.get(&name)?;
    let schedules = state.schedule_store.list()?;
    let triggers = state.trigger_store.list()?;
    let runs = state.orchestration_store.list_runs(None, 200)?;

    Ok(Json(build_workflow_detail(
        &name, &team, &schedules, &triggers, &runs, &state,
    )?))
}
