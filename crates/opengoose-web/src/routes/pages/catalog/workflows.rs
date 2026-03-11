use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use opengoose_teams::TeamStore;
use serde::Deserialize;
use tracing::error;

use crate::data::load_workflows_page;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

use super::render::{render_workflow_trigger_status, render_workflows_page};

#[derive(Deserialize, Default)]
pub(crate) struct WorkflowQuery {
    pub(crate) workflow: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TriggerWorkflowBody {
    input: Option<String>,
}

pub(crate) async fn workflows(
    State(state): State<PageState>,
    Query(query): Query<WorkflowQuery>,
) -> WebResult {
    let page = load_workflows_page(state.db, query.workflow).map_err(internal_error)?;
    render_workflows_page(page)
}

pub(crate) async fn trigger_workflow_action(
    State(state): State<PageState>,
    Path(name): Path<String>,
    body: Option<axum::Json<TriggerWorkflowBody>>,
) -> Result<Html<String>, (StatusCode, Html<String>)> {
    let input = body
        .and_then(|axum::Json(payload)| payload.input)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Manual run requested from the web dashboard for {name}"));

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
