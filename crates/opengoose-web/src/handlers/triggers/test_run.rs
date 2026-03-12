use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::error;

use crate::handlers::AppError;
use crate::state::AppState;

use super::TestTriggerRequest;
use super::validation::{resolve_test_input, trigger_not_found};

/// POST /api/triggers/:name/test — fire a test run for the trigger's workflow.
pub async fn test_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<TestTriggerRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let trigger = state
        .trigger_store
        .get_by_name(&name)?
        .ok_or_else(|| trigger_not_found(&name))?;

    let input = resolve_test_input(&trigger, body.map(|Json(payload)| payload), &name);

    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let trigger_store = state.trigger_store.clone();
    let team_name = trigger.team_name.clone();
    let trigger_name = trigger.name.clone();
    let run_input = input.clone();

    tokio::spawn(async move {
        match opengoose_teams::run_headless(opengoose_teams::HeadlessConfig::new(
            &team_name, &run_input, db, event_bus,
        ))
        .await
        {
            Ok((run_id, _)) => {
                if let Err(error) = trigger_store.mark_fired(&trigger_name) {
                    error!(
                        trigger = %trigger_name,
                        %error,
                        "failed to mark trigger fired after test"
                    );
                } else {
                    tracing::info!(trigger = %trigger_name, run_id, "test trigger run completed");
                }
            }
            Err(error) => {
                error!(trigger = %trigger_name, team = %team_name, %error, "test trigger run failed");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "accepted": true,
            "trigger": name,
            "team": trigger.team_name,
            "input": input,
        })),
    ))
}
