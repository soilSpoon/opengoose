use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::error;

use crate::handlers::AppError;
use crate::state::AppState;

use super::{TriggerWorkflowRequest, TriggerWorkflowResponse};

/// POST /api/workflows/:name/trigger — enqueue a background run for a workflow.
pub async fn trigger_workflow(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<TriggerWorkflowRequest>>,
) -> Result<(StatusCode, Json<TriggerWorkflowResponse>), AppError> {
    let team = state.team_store.get(&name)?;
    let input = manual_run_input(&name, body);

    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let workflow_name = name.clone();
    let workflow_input = input.clone();
    tokio::spawn(async move {
        if let Err(error) = opengoose_teams::run_headless(opengoose_teams::HeadlessConfig::new(
            &workflow_name,
            &workflow_input,
            db,
            event_bus,
        ))
        .await
        {
            error!(workflow = %workflow_name, %error, "manual workflow trigger failed");
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(TriggerWorkflowResponse {
            workflow: team.title,
            accepted: true,
            input,
        }),
    ))
}

fn manual_run_input(name: &str, body: Option<Json<TriggerWorkflowRequest>>) -> String {
    body.and_then(|Json(payload)| payload.input)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Manual run requested from the web dashboard for {name}"))
}
