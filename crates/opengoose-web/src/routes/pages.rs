use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_core::Stream;
use opengoose_persistence::Database;
use opengoose_types::SessionKey;
use serde::Deserialize;

use super::{PartialResult, WebResult, internal_error, render_partial, render_template};
use crate::data::{
    AgentDetailView, AgentsPageView, DashboardView, Notice, QueueDetailView, QueuePageView,
    RemoteAgentsPageView, RunDetailView, RunsPageView, ScheduleEditorView, ScheduleSaveInput,
    SchedulesPageView, SessionDetailView, SessionsPageView, TeamEditorView, TeamsPageView,
    TriggerDetailView, TriggersPageView, WorkflowDetailView, WorkflowsPageView, delete_schedule,
    load_agents_page, load_dashboard, load_queue_page, load_remote_agents_page, load_runs_page,
    load_schedules_page, load_sessions_page, load_teams_page, load_triggers_page,
    load_workflows_page, save_schedule, save_team_yaml, toggle_schedule,
};
use crate::server::PageState;

#[derive(Deserialize, Default)]
pub(crate) struct SessionQuery {
    session: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionModelForm {
    intent: String,
    session_key: String,
    selected_model: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RunQuery {
    pub(crate) run: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct AgentQuery {
    agent: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TeamQuery {
    team: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct WorkflowQuery {
    workflow: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct ScheduleQuery {
    schedule: Option<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct TriggerQuery {
    trigger: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TeamSaveForm {
    original_name: String,
    yaml: String,
}

#[derive(Deserialize)]
pub(crate) struct ScheduleActionForm {
    intent: String,
    original_name: Option<String>,
    name: Option<String>,
    cron_expression: Option<String>,
    team_name: Option<String>,
    input: Option<String>,
    enabled: Option<String>,
    confirm_delete: Option<String>,
}

pub(crate) fn router(state: PageState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard/events", get(dashboard_events))
        .route("/sessions", get(sessions).post(session_model_action))
        .route("/runs", get(runs))
        .route("/agents", get(agents))
        .route("/remote-agents", get(remote_agents))
        .route("/remote-agents/events", get(remote_agents_events))
        .route("/workflows", get(workflows))
        .route("/schedules", get(schedules).post(schedule_action))
        .route("/triggers", get(triggers))
        .route("/teams", get(teams).post(team_save))
        .route("/queue", get(queue))
        .with_state(state)
}

pub(crate) async fn dashboard(State(state): State<PageState>) -> WebResult {
    let dashboard = load_dashboard(state.db.clone()).map_err(internal_error)?;
    let live_html = render_partial(&DashboardLiveTemplate {
        dashboard: dashboard.clone(),
    })?;
    render_template(&DashboardTemplate {
        page_title: "OpenGoose Dashboard",
        current_nav: "dashboard",
        dashboard,
        live_html,
    })
}

pub(crate) async fn dashboard_events(
    State(state): State<PageState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let initial = render_dashboard_live_html(db.clone())?;
    let event_stream = stream! {
        yield Ok(datastar_patch_event("#dashboard-live", "inner", &initial));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match render_dashboard_live_html(db.clone()) {
                Ok(html) => yield Ok(datastar_patch_event("#dashboard-live", "inner", &html)),
                Err(_) => {
                    let fallback = dashboard_stream_error_html();
                    yield Ok(datastar_patch_event("#dashboard-live", "inner", fallback));
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-dashboard"),
    ))
}

pub(crate) async fn remote_agents_events()
-> Sse<impl Stream<Item = Result<Event, Infallible>> + Send> {
    let event_stream = stream! {
        yield Ok(Event::default().data("remote-agents-ready"));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            yield Ok(Event::default().data("remote-agents-refresh"));
        }
    };

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-remote-agents"),
    )
}

pub(crate) async fn sessions(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> WebResult {
    let page = load_sessions_page(state.db, query.session).map_err(internal_error)?;
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SessionsTemplate {
        page_title: "Sessions",
        current_nav: "sessions",
        page,
        detail_html,
    })
}

pub(crate) async fn session_model_action(
    State(state): State<PageState>,
    Form(form): Form<SessionModelForm>,
) -> WebResult {
    let session_key = form.session_key.trim();
    if session_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Html("Session key is required.".into()),
        ));
    }

    let normalized_model = form
        .selected_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let notice_text = match form.intent.as_str() {
        "save" => {
            opengoose_persistence::SessionStore::new(state.db.clone())
                .set_selected_model(
                    &SessionKey::from_stable_id(session_key),
                    normalized_model.as_deref(),
                )
                .map_err(|error| internal_error(error.into()))?;
            match normalized_model.as_deref() {
                Some(model) => format!("Session model set to `{model}`."),
                None => "Session model override cleared.".into(),
            }
        }
        "clear" => {
            opengoose_persistence::SessionStore::new(state.db.clone())
                .set_selected_model(&SessionKey::from_stable_id(session_key), None)
                .map_err(|error| internal_error(error.into()))?;
            "Session model override cleared.".into()
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Html("Unsupported session action.".into()),
            ));
        }
    };

    let mut page =
        load_sessions_page(state.db, Some(session_key.to_string())).map_err(internal_error)?;
    page.selected.notice = Some(Notice {
        text: notice_text,
        tone: "success",
    });
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SessionsTemplate {
        page_title: "Sessions",
        current_nav: "sessions",
        page,
        detail_html,
    })
}

pub(crate) async fn runs(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_runs_page(state.db, query.run).map_err(internal_error)?;
    let detail_html = render_partial(&RunDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&RunsTemplate {
        page_title: "Runs",
        current_nav: "runs",
        page,
        detail_html,
    })
}

pub(crate) async fn agents(Query(query): Query<AgentQuery>) -> WebResult {
    let page = load_agents_page(query.agent).map_err(internal_error)?;
    let detail_html = render_partial(&AgentDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&AgentsTemplate {
        page_title: "Agents",
        current_nav: "agents",
        page,
        detail_html,
    })
}

pub(crate) async fn remote_agents(State(state): State<PageState>, headers: HeaderMap) -> WebResult {
    let page = load_remote_agents_page(&state.remote_registry, websocket_url(&headers))
        .await
        .map_err(internal_error)?;
    let live_html = render_partial(&RemoteAgentsLiveTemplate { page: page.clone() })?;

    render_template(&RemoteAgentsTemplate {
        page_title: "Remote Agents",
        current_nav: "remote_agents",
        page,
        live_html,
    })
}

pub(crate) async fn workflows(
    State(state): State<PageState>,
    Query(query): Query<WorkflowQuery>,
) -> WebResult {
    let page = load_workflows_page(state.db, query.workflow).map_err(internal_error)?;
    let detail_html = render_partial(&WorkflowDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&WorkflowsTemplate {
        page_title: "Workflows",
        current_nav: "workflows",
        page,
        detail_html,
    })
}

pub(crate) async fn schedules(
    State(state): State<PageState>,
    Query(query): Query<ScheduleQuery>,
) -> WebResult {
    let page = load_schedules_page(state.db, query.schedule).map_err(internal_error)?;
    let detail_html = render_partial(&ScheduleDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SchedulesTemplate {
        page_title: "Schedules",
        current_nav: "schedules",
        page,
        detail_html,
    })
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
                StatusCode::BAD_REQUEST,
                Html("Unsupported schedule action.".into()),
            ));
        }
    }
    .map_err(internal_error)?;

    let detail_html = render_partial(&ScheduleDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SchedulesTemplate {
        page_title: "Schedules",
        current_nav: "schedules",
        page,
        detail_html,
    })
}

pub(crate) async fn triggers(
    State(state): State<PageState>,
    Query(query): Query<TriggerQuery>,
) -> WebResult {
    let page = load_triggers_page(state.db, query.trigger).map_err(internal_error)?;
    let detail_html = render_partial(&TriggerDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&TriggersTemplate {
        page_title: "Triggers",
        current_nav: "triggers",
        page,
        detail_html,
    })
}

pub(crate) async fn teams(Query(query): Query<TeamQuery>) -> WebResult {
    let page = load_teams_page(query.team).map_err(internal_error)?;
    let detail_html = render_partial(&TeamEditorTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&TeamsTemplate {
        page_title: "Teams",
        current_nav: "teams",
        page,
        detail_html,
    })
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
    let detail_html = render_partial(&TeamEditorTemplate { detail })?;

    render_template(&TeamsTemplate {
        page_title: "Teams",
        current_nav: "teams",
        page,
        detail_html,
    })
}

pub(crate) async fn queue(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_queue_page(state.db, query.run).map_err(internal_error)?;
    let detail_html = render_partial(&QueueDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&QueueTemplate {
        page_title: "Queue",
        current_nav: "queue",
        page,
        detail_html,
    })
}

fn render_dashboard_live_html(db: Arc<Database>) -> PartialResult {
    let dashboard = load_dashboard(db).map_err(internal_error)?;
    render_partial(&DashboardLiveTemplate { dashboard })
}

fn websocket_url(headers: &HeaderMap) -> String {
    let host = forwarded_header(headers, "x-forwarded-host")
        .or_else(|| forwarded_host(headers))
        .or_else(|| header_string(headers, "host"))
        .unwrap_or_else(|| "localhost:3000".into());
    let scheme = match forwarded_header(headers, "x-forwarded-proto")
        .or_else(|| forwarded_proto(headers))
        .as_deref()
    {
        Some("https") | Some("wss") => "wss",
        _ => "ws",
    };

    format!("{scheme}://{host}/api/agents/connect")
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn forwarded_header(headers: &HeaderMap, name: &str) -> Option<String> {
    header_string(headers, name)
}

fn forwarded_proto(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("proto="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

fn forwarded_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("host="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

fn datastar_patch_event(selector: &str, mode: &str, html: &str) -> Event {
    let mut payload = format!("selector {selector}\nmode {mode}");
    if html.is_empty() {
        payload.push_str("\nelements ");
    } else {
        for line in html.lines() {
            payload.push('\n');
            payload.push_str("elements ");
            payload.push_str(line);
        }
    }

    Event::default()
        .event("datastar-patch-elements")
        .data(payload)
}

fn dashboard_stream_error_html() -> &'static str {
    r#"
<section class="callout tone-danger">
  <p class="eyebrow">Stream degraded</p>
  <h2>Dashboard snapshot unavailable</h2>
  <p>The live board is retrying in the background. The rest of the page remains server-rendered and usable.</p>
</section>
"#
}

/// Render the dashboard live partial from a pre-built `DashboardView`.
///
/// Exposed for benchmarking. Returns the rendered HTML string or an error message.
pub fn render_dashboard_live_partial(
    dashboard: crate::data::DashboardView,
) -> Result<String, String> {
    DashboardLiveTemplate { dashboard }
        .render()
        .map_err(|e| e.to_string())
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    dashboard: DashboardView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/dashboard_live.html")]
struct DashboardLiveTemplate {
    dashboard: DashboardView,
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
#[template(path = "remote_agents.html")]
struct RemoteAgentsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: RemoteAgentsPageView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/remote_agents_live.html")]
struct RemoteAgentsLiveTemplate {
    page: RemoteAgentsPageView,
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

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::data::{
        DashboardView, QueueDetailView, ScheduleEditorView, SchedulesPageView, SessionDetailView,
        SessionsPageView, WorkflowDetailView, WorkflowsPageView,
    };

    pub(crate) fn render_dashboard_live(dashboard: DashboardView) -> PartialResult {
        render_partial(&DashboardLiveTemplate { dashboard })
    }

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

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request};
    use opengoose_persistence::{
        Database, OrchestrationStore, ScheduleStore, SessionStore, TriggerStore,
    };
    use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};
    use opengoose_types::{ChannelMetricsStore, EventBus, Platform, SessionKey};
    use tower::ServiceExt;

    use super::*;
    use crate::test_support::with_temp_home;

    fn save_team(name: &str) {
        TeamStore::new()
            .expect("team store should open")
            .save(
                &TeamDefinition {
                    version: "1.0.0".into(),
                    title: name.into(),
                    description: Some(format!("{name} team")),
                    workflow: OrchestrationPattern::Chain,
                    agents: vec![TeamAgent {
                        profile: "tester".into(),
                        role: Some("validate setup".into()),
                    }],
                    router: None,
                    fan_out: None,
                },
                true,
            )
            .expect("team should save");
    }

    fn page_state(db: Arc<Database>) -> PageState {
        PageState {
            db,
            remote_registry: RemoteAgentRegistry::new(RemoteConfig::default()),
            channel_metrics: ChannelMetricsStore::new(),
            _event_bus: EventBus::new(256),
        }
    }

    fn run_async(test: impl Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build")
            .block_on(test);
    }

    fn save_session(db: Arc<Database>, key: &SessionKey, active_team: Option<&str>) {
        let store = SessionStore::new(db);
        store
            .append_user_message(key, "Need a reviewer on this run.", Some("tester"))
            .expect("session should seed");
        if let Some(team) = active_team {
            store
                .set_active_team(key, Some(team))
                .expect("active team should seed");
        }
    }

    fn save_run(db: Arc<Database>, run_id: &str) {
        OrchestrationStore::new(db)
            .create_run(
                run_id,
                "discord:ns:ops:chan-1",
                "ops",
                "chain",
                "Review the latest deploy.",
                3,
            )
            .expect("run should seed");
    }

    async fn read_body(response: axum::response::Response) -> String {
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body should be readable");
        String::from_utf8(body.to_vec()).expect("response body should be utf-8")
    }

    #[test]
    fn page_router_get_routes_return_expected_statuses() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let app = router(page_state(Arc::new(
                    Database::open_in_memory().expect("db should open"),
                )));

                for path in [
                    "/",
                    "/dashboard/events",
                    "/sessions",
                    "/runs",
                    "/agents",
                    "/remote-agents",
                    "/remote-agents/events",
                    "/workflows",
                    "/schedules",
                    "/triggers",
                    "/teams",
                    "/queue",
                ] {
                    let response = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .method(Method::GET)
                                .uri(path)
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .expect("request should be handled");

                    assert_eq!(
                        response.status(),
                        StatusCode::OK,
                        "path `{path}` should render"
                    );
                }
            });
        });
    }

    #[test]
    fn page_router_post_routes_return_expected_statuses() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let app = router(page_state(Arc::new(
                    Database::open_in_memory().expect("db should open"),
                )));

                let schedule_response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(Method::POST)
                            .uri("/schedules")
                            .header("content-type", "application/x-www-form-urlencoded")
                            .body(Body::from("intent=unsupported"))
                            .unwrap(),
                    )
                    .await
                    .expect("schedule request should be handled");
                assert_eq!(schedule_response.status(), StatusCode::BAD_REQUEST);

                let team_response = app
                    .oneshot(
                        Request::builder()
                            .method(Method::POST)
                            .uri("/teams")
                            .header("content-type", "application/x-www-form-urlencoded")
                            .body(Body::from("original_name=broken&yaml=title%3A+broken"))
                            .unwrap(),
                    )
                    .await
                    .expect("team request should be handled");
                assert_eq!(team_response.status(), StatusCode::OK);
            });
        });
    }

    #[tokio::test]
    async fn dashboard_handler_renders_mock_preview() {
        let Html(html) = dashboard(State(page_state(Arc::new(
            Database::open_in_memory().expect("db should open"),
        ))))
        .await
        .expect("handler should render");

        assert!(html.contains("Mock preview"));
        assert!(html.contains("No runtime data yet"));
    }

    #[tokio::test]
    async fn sessions_handler_invalid_selection_falls_back_to_live_session() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        let session_key = SessionKey::new(Platform::Discord, "ops", "chan-1");
        save_session(db.clone(), &session_key, Some("reviewers"));

        let Html(html) = sessions(
            State(page_state(db)),
            Query(SessionQuery {
                session: Some("discord:ns:missing:session".into()),
            }),
        )
        .await
        .expect("handler should render");

        assert!(html.contains("Live runtime"));
        assert!(html.contains("Session chan-1"));
        assert!(html.contains(&session_key.to_stable_id()));
    }

    #[tokio::test]
    async fn runs_handler_invalid_selection_falls_back_to_live_run() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        save_run(db.clone(), "run-live-1");

        let Html(html) = runs(
            State(page_state(db)),
            Query(RunQuery {
                run: Some("missing-run".into()),
            }),
        )
        .await
        .expect("handler should render");

        assert!(html.contains("Live runtime"));
        assert!(html.contains("Run run-live-1"));
        assert!(html.contains("ops / chain"));
    }

    #[test]
    fn agents_handler_renders_bundled_defaults_for_unknown_selection() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let Html(html) = agents(Query(AgentQuery {
                    agent: Some("missing-agent".into()),
                }))
                .await
                .expect("handler should render");

                assert!(html.contains("Bundled defaults"));
                assert!(html.contains("aria-current=\"page\""));
            });
        });
    }

    #[test]
    fn workflows_handler_renders_bundled_defaults_for_unknown_selection() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let Html(html) = workflows(
                    State(page_state(Arc::new(
                        Database::open_in_memory().expect("db should open"),
                    ))),
                    Query(WorkflowQuery {
                        workflow: Some("missing-workflow".into()),
                    }),
                )
                .await
                .expect("handler should render");

                assert!(html.contains("Bundled defaults"));
                assert!(html.contains("Workflow detail"));
                assert!(html.contains("aria-current=\"page\""));
            });
        });
    }

    #[tokio::test]
    async fn triggers_handler_invalid_selection_falls_back_to_existing_trigger() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        TriggerStore::new(db.clone())
            .create("incoming", "webhook_received", "{}", "ops", "")
            .expect("trigger should seed");

        let Html(html) = triggers(
            State(page_state(db)),
            Query(TriggerQuery {
                trigger: Some("missing-trigger".into()),
            }),
        )
        .await
        .expect("handler should render");

        assert!(html.contains("1 trigger(s)"));
        assert!(html.contains("incoming"));
        assert!(html.contains("webhook_received"));
    }

    #[tokio::test]
    async fn queue_handler_invalid_selection_falls_back_to_live_run() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        save_run(db.clone(), "run-queue-1");

        let Html(html) = queue(
            State(page_state(db)),
            Query(RunQuery {
                run: Some("missing-run".into()),
            }),
        )
        .await
        .expect("handler should render");

        assert!(html.contains("Live runtime"));
        assert!(html.contains("Queue run-queue-1"));
        assert!(html.contains("No queue traffic has been recorded for this run yet."));
    }

    #[test]
    fn team_save_invalid_yaml_renders_editor_error_notice() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let Html(html) = team_save(Form(TeamSaveForm {
                    original_name: "broken-team".into(),
                    yaml: "title: broken-team".into(),
                }))
                .await
                .expect("handler should render");

                assert!(html.contains("Fix the YAML validation error and try again."));
                assert!(html.contains("Editor draft"));
            });
        });
    }

    #[test]
    fn schedule_action_missing_team_renders_validation_notice() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let Html(html) = schedule_action(
                    State(page_state(Arc::new(
                        Database::open_in_memory().expect("db should open"),
                    ))),
                    Form(ScheduleActionForm {
                        intent: "save".into(),
                        original_name: None,
                        name: Some("nightly-ops".into()),
                        cron_expression: Some("0 0 * * * *".into()),
                        team_name: Some("missing-team".into()),
                        input: Some(String::new()),
                        enabled: Some("yes".into()),
                        confirm_delete: None,
                    }),
                )
                .await
                .expect("handler should render");

                assert!(html.contains("The selected team is not installed."));
                assert!(html.contains("nightly-ops"));
            });
        });
    }

    #[test]
    fn schedules_handler_renders_existing_schedule() {
        with_temp_home("opengoose-routes-pages-home", || {
            save_team("ops");
            let db = Arc::new(Database::open_in_memory().expect("db should open"));
            ScheduleStore::new(db.clone())
                .create(
                    "nightly-ops",
                    "0 0 * * * *",
                    "ops",
                    "",
                    Some("2026-03-11 00:00:00"),
                )
                .expect("schedule should seed");

            run_async(async {
                let Html(html) = schedules(
                    State(page_state(db)),
                    Query(ScheduleQuery {
                        schedule: Some("nightly-ops".into()),
                    }),
                )
                .await
                .expect("handler should render");

                assert!(html.contains("nightly-ops"));
                assert!(html.contains("Recent matching runs"));
            });
        });
    }

    #[test]
    fn schedule_action_creates_schedule_from_form_post() {
        with_temp_home("opengoose-routes-pages-home", || {
            save_team("ops");
            let db = Arc::new(Database::open_in_memory().expect("db should open"));
            run_async(async {
                let Html(html) = schedule_action(
                    State(page_state(db.clone())),
                    Form(ScheduleActionForm {
                        intent: "save".into(),
                        original_name: None,
                        name: Some("nightly-ops".into()),
                        cron_expression: Some("0 0 * * * *".into()),
                        team_name: Some("ops".into()),
                        input: Some(String::new()),
                        enabled: Some("yes".into()),
                        confirm_delete: None,
                    }),
                )
                .await
                .expect("save action should render");

                assert!(html.contains("Schedule created."));
                assert!(
                    ScheduleStore::new(db)
                        .get_by_name("nightly-ops")
                        .expect("lookup should succeed")
                        .is_some()
                );
            });
        });
    }

    #[test]
    fn schedule_action_unsupported_intent_returns_bad_request() {
        with_temp_home("opengoose-routes-pages-home", || {
            run_async(async {
                let response = router(page_state(Arc::new(
                    Database::open_in_memory().expect("db should open"),
                )))
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/schedules")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("intent=unsupported"))
                        .unwrap(),
                )
                .await
                .expect("request should be handled");

                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                let html = read_body(response).await;
                assert!(html.contains("Unsupported schedule action."));
            });
        });
    }

    #[tokio::test]
    async fn remote_agents_handler_renders_empty_registry() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "opengoose.test".parse().expect("host header"));

        let Html(html) = remote_agents(
            State(page_state(Arc::new(
                Database::open_in_memory().expect("db should open"),
            ))),
            headers,
        )
        .await
        .expect("handler should render");

        assert!(html.contains("No remote agents are connected right now."));
        assert!(html.contains("ws://opengoose.test/api/agents/connect"));
        assert!(html.contains("data-live-events-url=\"/remote-agents/events\""));
    }

    #[tokio::test]
    async fn remote_agents_handler_renders_registered_agents() {
        let state = page_state(Arc::new(
            Database::open_in_memory().expect("db should open"),
        ));
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .remote_registry
            .register(
                "remote-a".into(),
                vec!["execute".into(), "relay".into()],
                "ws://remote-a:9000".into(),
                tx,
            )
            .await
            .expect("agent should register");

        let mut headers = HeaderMap::new();
        headers.insert("host", "dashboard.local".parse().expect("host header"));

        let Html(html) = remote_agents(State(state), headers)
            .await
            .expect("handler should render");

        assert!(html.contains("remote-a"));
        assert!(html.contains("execute"));
        assert!(html.contains("ws://remote-a:9000"));
        assert!(html.contains("/api/agents/remote/remote-a"));
        assert!(html.contains("Disconnect"));
    }

    #[test]
    fn websocket_url_prefers_forwarded_https_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            "goose.example.com".parse().expect("forwarded host"),
        );
        headers.insert(
            "x-forwarded-proto",
            "https".parse().expect("forwarded proto"),
        );
        headers.insert("host", "localhost:3000".parse().expect("host header"));

        assert_eq!(
            websocket_url(&headers),
            "wss://goose.example.com/api/agents/connect"
        );
    }
}
