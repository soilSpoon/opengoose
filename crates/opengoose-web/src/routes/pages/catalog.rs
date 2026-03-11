use askama::Template;
use async_stream::stream;
use axum::extract::{Form, Path, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_teams::TeamStore;
use opengoose_types::AppEventKind;
use serde::Deserialize;
use std::convert::Infallible;
use tracing::error;

use crate::data::{
    AgentDetailView, AgentsPageView, QueueDetailView, QueuePageView, RunDetailView, RunsPageView,
    ScheduleEditorView, ScheduleSaveInput, SchedulesPageView, SessionDetailView, SessionsPageView,
    TeamEditorView, TeamsPageView, TriggerDetailView, TriggersPageView, WorkflowDetailView,
    WorkflowsPageView, delete_schedule, load_agents_page, load_queue_page, load_runs_page,
    load_schedules_page, load_sessions_page, load_teams_page, load_triggers_page,
    load_workflows_page, save_schedule, save_team_yaml, toggle_schedule,
};
use crate::routes::{
    PartialResult, WebResult, datastar_patch_elements_event, internal_error, render_partial,
    render_template,
};
use crate::server::PageState;

macro_rules! render_catalog_page {
    ($page_title:literal, $current_nav:literal, $page:expr, $page_template:ident, $detail_template:ident) => {{
        let page = $page;
        let detail_html = render_partial(&$detail_template {
            detail: page.selected.clone(),
        })?;

        render_template(&$page_template {
            page_title: $page_title,
            current_nav: $current_nav,
            page,
            detail_html,
        })
    }};
}

#[derive(Deserialize, Default)]
pub(crate) struct SessionQuery {
    pub(crate) session: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RunQuery {
    pub(crate) run: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct AgentQuery {
    pub(crate) agent: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TeamQuery {
    pub(crate) team: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct WorkflowQuery {
    pub(crate) workflow: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct ScheduleQuery {
    pub(crate) schedule: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TriggerQuery {
    pub(crate) trigger: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TeamSaveForm {
    pub(crate) original_name: String,
    pub(crate) yaml: String,
}

#[derive(Deserialize)]
pub(crate) struct ScheduleActionForm {
    pub(crate) intent: String,
    pub(crate) original_name: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) cron_expression: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) input: Option<String>,
    pub(crate) enabled: Option<String>,
    pub(crate) confirm_delete: Option<String>,
}

pub(crate) async fn sessions(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> WebResult {
    let page = load_sessions_page(state.db, query.session).map_err(internal_error)?;
    render_catalog_page!(
        "Sessions",
        "sessions",
        page,
        SessionsTemplate,
        SessionDetailTemplate
    )
}

pub(crate) async fn sessions_events(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let selected = query.session;
    let mut rx = state.event_bus.subscribe();
    let initial = render_sessions_stream_html(db.clone(), selected.clone())?;

    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        loop {
            match rx.recv().await {
                Ok(app_event) if matches_sessions_live_event(&app_event.kind) => {
                    match render_sessions_stream_html(db.clone(), selected.clone()) {
                        Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                        Err(_) => yield Ok(datastar_patch_elements_event(sessions_stream_error_html())),
                    }
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("opengoose-sessions"),
    ))
}

pub(crate) async fn runs(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_runs_page(state.db, query.run).map_err(internal_error)?;
    render_catalog_page!("Runs", "runs", page, RunsTemplate, RunDetailTemplate)
}

pub(crate) async fn agents(Query(query): Query<AgentQuery>) -> WebResult {
    let page = load_agents_page(query.agent).map_err(internal_error)?;
    render_catalog_page!(
        "Agents",
        "agents",
        page,
        AgentsTemplate,
        AgentDetailTemplate
    )
}

pub(crate) async fn workflows(
    State(state): State<PageState>,
    Query(query): Query<WorkflowQuery>,
) -> WebResult {
    let page = load_workflows_page(state.db, query.workflow).map_err(internal_error)?;
    render_catalog_page!(
        "Workflows",
        "workflows",
        page,
        WorkflowsTemplate,
        WorkflowDetailTemplate
    )
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

pub(crate) async fn schedules(
    State(state): State<PageState>,
    Query(query): Query<ScheduleQuery>,
) -> WebResult {
    let page = load_schedules_page(state.db, query.schedule).map_err(internal_error)?;
    render_catalog_page!(
        "Schedules",
        "schedules",
        page,
        SchedulesTemplate,
        ScheduleDetailTemplate
    )
}

pub(crate) async fn schedule_action(
    State(state): State<PageState>,
    Form(form): Form<ScheduleActionForm>,
) -> WebResult {
    let target_name = form
        .original_name
        .clone()
        .or_else(|| form.name.clone())
        .unwrap_or_default();
    let page = match form.intent.as_str() {
        "save" => save_schedule(
            state.db,
            ScheduleSaveInput {
                original_name: form.original_name,
                name: form.name.unwrap_or_default(),
                cron_expression: form.cron_expression.unwrap_or_default(),
                team_name: form.team_name.unwrap_or_default(),
                input: form.input.unwrap_or_default(),
                enabled: form.enabled.is_some(),
            },
        ),
        "toggle" => toggle_schedule(state.db, target_name),
        "delete" => delete_schedule(
            state.db,
            target_name,
            form.confirm_delete.as_deref() == Some("yes"),
        ),
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported schedule action.".into()),
            ));
        }
    }
    .map_err(internal_error)?;

    render_catalog_page!(
        "Schedules",
        "schedules",
        page,
        SchedulesTemplate,
        ScheduleDetailTemplate
    )
}

pub(crate) async fn triggers(
    State(state): State<PageState>,
    Query(query): Query<TriggerQuery>,
) -> WebResult {
    let page = load_triggers_page(state.db, query.trigger).map_err(internal_error)?;
    render_catalog_page!(
        "Triggers",
        "triggers",
        page,
        TriggersTemplate,
        TriggerDetailTemplate
    )
}

pub(crate) async fn teams(Query(query): Query<TeamQuery>) -> WebResult {
    let page = load_teams_page(query.team).map_err(internal_error)?;
    render_catalog_page!("Teams", "teams", page, TeamsTemplate, TeamEditorTemplate)
}

pub(crate) async fn team_save(Form(form): Form<TeamSaveForm>) -> WebResult {
    let original_name = form.original_name.clone();
    let detail = save_team_yaml(form.original_name, form.yaml).map_err(internal_error)?;
    let active_team = match detail.notice.as_ref().map(|notice| notice.tone) {
        Some("success") => detail.title.clone(),
        _ => original_name,
    };

    let mut page = load_teams_page(Some(active_team)).map_err(internal_error)?;
    page.selected = detail.clone();

    render_catalog_page!("Teams", "teams", page, TeamsTemplate, TeamEditorTemplate)
}

pub(crate) async fn queue(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_queue_page(state.db, query.run).map_err(internal_error)?;
    render_catalog_page!("Queue", "queue", page, QueueTemplate, QueueDetailTemplate)
}

#[derive(Deserialize, Default)]
pub(crate) struct TriggerWorkflowBody {
    pub(crate) input: Option<String>,
}

#[derive(Template)]
#[template(path = "sessions.html")]
struct SessionsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: SessionsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/session_detail.html")]
struct SessionDetailTemplate {
    detail: SessionDetailView,
}

#[derive(Template)]
#[template(path = "partials/sessions_page_intro.html")]
struct SessionsPageIntroTemplate {
    page: SessionsPageView,
}

#[derive(Template)]
#[template(path = "partials/sessions_shell.html")]
struct SessionsShellTemplate {
    page: SessionsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "runs.html")]
struct RunsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: RunsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/run_detail.html")]
struct RunDetailTemplate {
    detail: RunDetailView,
}

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: AgentsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/agent_detail.html")]
struct AgentDetailTemplate {
    detail: AgentDetailView,
}

#[derive(Template)]
#[template(path = "workflows.html")]
struct WorkflowsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: WorkflowsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/workflow_detail.html")]
struct WorkflowDetailTemplate {
    detail: WorkflowDetailView,
}

#[derive(Template)]
#[template(path = "partials/workflow_trigger_status.html")]
struct WorkflowTriggerStatusTemplate {
    message: String,
    tone: &'static str,
}

#[derive(Template)]
#[template(path = "schedules.html")]
struct SchedulesTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: SchedulesPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/schedule_detail.html")]
struct ScheduleDetailTemplate {
    detail: ScheduleEditorView,
}

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: TriggersPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/trigger_detail.html")]
struct TriggerDetailTemplate {
    detail: TriggerDetailView,
}

#[derive(Template)]
#[template(path = "teams.html")]
struct TeamsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: TeamsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/team_editor.html")]
struct TeamEditorTemplate {
    detail: TeamEditorView,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: QueuePageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/queue_detail.html")]
struct QueueDetailTemplate {
    detail: QueueDetailView,
}

fn render_sessions_stream_html(
    db: std::sync::Arc<opengoose_persistence::Database>,
    selected: Option<String>,
) -> PartialResult {
    let page = load_sessions_page(db, selected).map_err(internal_error)?;
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;
    let intro_html = render_partial(&SessionsPageIntroTemplate { page: page.clone() })?;
    let shell_html = render_partial(&SessionsShellTemplate { page, detail_html })?;
    Ok(format!("{intro_html}{shell_html}"))
}

fn render_workflow_trigger_status(message: String, tone: &'static str) -> PartialResult {
    render_partial(&WorkflowTriggerStatusTemplate { message, tone })
}

fn matches_sessions_live_event(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::SessionUpdated { .. }
            | AppEventKind::MessageReceived { .. }
            | AppEventKind::ResponseSent { .. }
            | AppEventKind::PairingCompleted { .. }
            | AppEventKind::TeamActivated { .. }
            | AppEventKind::TeamDeactivated { .. }
            | AppEventKind::SessionDisconnected { .. }
            | AppEventKind::StreamStarted { .. }
            | AppEventKind::StreamUpdated { .. }
            | AppEventKind::StreamCompleted { .. }
            | AppEventKind::RunUpdated { .. }
            | AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
    )
}

fn sessions_stream_error_html() -> &'static str {
    r#"
<section id="detail-shell" class="detail-shell">
  <section class="detail-frame">
    <section class="callout tone-danger">
      <p class="eyebrow">Session stream degraded</p>
      <h2>Live session updates paused</h2>
      <p>The page will reconnect automatically when new runtime events arrive.</p>
    </section>
  </section>
</section>
"#
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::data::{
        QueueDetailView, ScheduleEditorView, SchedulesPageView, SessionDetailView,
        SessionsPageView, WorkflowDetailView, WorkflowsPageView,
    };
    use crate::routes::PartialResult;

    pub(crate) fn render_session_detail(detail: SessionDetailView) -> PartialResult {
        render_partial(&SessionDetailTemplate { detail })
    }

    pub(crate) fn render_sessions_page(
        page: SessionsPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&SessionsTemplate {
            page_title: "Sessions",
            current_nav: "sessions",
            page,
            detail_html,
        })
    }

    pub(crate) fn render_queue_detail(detail: QueueDetailView) -> PartialResult {
        render_partial(&QueueDetailTemplate { detail })
    }

    pub(crate) fn render_schedule_detail(detail: ScheduleEditorView) -> PartialResult {
        render_partial(&ScheduleDetailTemplate { detail })
    }

    pub(crate) fn render_schedules_page(
        page: SchedulesPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&SchedulesTemplate {
            page_title: "Schedules",
            current_nav: "schedules",
            page,
            detail_html,
        })
    }

    pub(crate) fn render_workflow_detail(detail: WorkflowDetailView) -> PartialResult {
        render_partial(&WorkflowDetailTemplate { detail })
    }

    pub(crate) fn render_workflows_page(
        page: WorkflowsPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&WorkflowsTemplate {
            page_title: "Workflows",
            current_nav: "workflows",
            page,
            detail_html,
        })
    }
}
