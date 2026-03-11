use axum::extract::{Path, State};
use axum::response::Html;
use opengoose_teams::TeamStore;
use tracing::error;

use crate::routes::pages::catalog_forms::TriggerWorkflowBody;
use crate::routes::pages::catalog_templates::render_workflow_trigger_status;
use crate::server::PageState;

pub(crate) async fn trigger_workflow_action(
    State(state): State<PageState>,
    Path(name): Path<String>,
    body: Option<axum::Json<TriggerWorkflowBody>>,
) -> Result<Html<String>, (axum::http::StatusCode, Html<String>)> {
    let input = resolve_workflow_input(body, &name);

    let team_store = match TeamStore::new() {
        Ok(store) => store,
        Err(error) => {
            let html = render_workflow_trigger_status(
                format!("Unable to load workflows: {error}"),
                "danger",
            )?;
            return Ok(Html(html));
        }
    };

    let team = match team_store.get(&name) {
        Ok(team) => team,
        Err(error) => {
            let html = render_workflow_trigger_status(
                format!("Workflow trigger failed: {error}"),
                "danger",
            )?;
            return Ok(Html(html));
        }
    };

    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let workflow_name = name.clone();
    let workflow_input = input.clone();
    tokio::spawn(async move {
        if let Err(error) =
            opengoose_teams::run_headless(&workflow_name, &workflow_input, db, event_bus).await
        {
            error!(workflow = %workflow_name, %error, "manual workflow trigger failed");
        }
    });

    Ok(Html(render_workflow_trigger_status(
        format!("{} queued. Check Runs for live progress.", team.title),
        "success",
    )?))
}

fn resolve_workflow_input(
    body: Option<axum::Json<TriggerWorkflowBody>>,
    workflow_name: &str,
) -> String {
    body.and_then(|axum::Json(payload)| payload.input)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_workflow_input(workflow_name))
}

fn default_workflow_input(workflow_name: &str) -> String {
    format!("Manual run requested from the web dashboard for {workflow_name}")
}
