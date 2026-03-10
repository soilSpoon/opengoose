use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, RunStatus, SessionStore};
use serde::{Deserialize, Serialize};

use crate::PageState;
use crate::data::{
    AgentDetailView, AgentsPageView, DashboardView, QueueDetailView, QueuePageView, RunDetailView,
    RunsPageView, ScheduleEditorView, ScheduleSaveInput, SchedulesPageView, SessionDetailView,
    SessionsPageView, TeamEditorView, TeamsPageView, TriggerDetailView, TriggersPageView,
    WorkflowDetailView, WorkflowsPageView, delete_schedule, load_agents_page, load_dashboard,
    load_queue_page, load_runs_page, load_schedules_page, load_sessions_page, load_teams_page,
    load_triggers_page, load_workflows_page, save_schedule, save_team_yaml, toggle_schedule,
};

// --- Result types ---

type WebResult = Result<Html<String>, (StatusCode, Html<String>)>;
type PartialResult = Result<String, (StatusCode, Html<String>)>;
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

// --- Query parameter structs ---

#[derive(Deserialize, Default)]
pub(crate) struct SessionQuery {
    session: Option<String>,
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

// --- Page handlers ---

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

// --- JSON API handlers ---

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
pub(crate) struct SessionMetrics {
    pub(crate) total: i64,
    pub(crate) messages: i64,
}

#[derive(Serialize)]
pub(crate) struct QueueMetrics {
    pub(crate) pending: i64,
    pub(crate) processing: i64,
    pub(crate) completed: i64,
    pub(crate) failed: i64,
    pub(crate) dead: i64,
}

#[derive(Serialize)]
pub(crate) struct RunMetrics {
    pub(crate) running: usize,
    pub(crate) completed: usize,
    pub(crate) failed: usize,
    pub(crate) suspended: usize,
}

#[derive(Serialize)]
pub(crate) struct MetricsResponse {
    pub(crate) sessions: SessionMetrics,
    pub(crate) queue: QueueMetrics,
    pub(crate) runs: RunMetrics,
}

pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub(crate) async fn metrics(State(state): State<PageState>) -> ApiResult<MetricsResponse> {
    let db = state.db;

    let session_stats = SessionStore::new(db.clone())
        .stats()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let queue_stats = MessageQueue::new(db.clone())
        .stats()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let run_store = OrchestrationStore::new(db);
    let recent_runs = run_store
        .list_runs(None, 200)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let running = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Running)
        .count();
    let completed = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Completed)
        .count();
    let failed = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Failed)
        .count();
    let suspended = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Suspended)
        .count();

    Ok(Json(MetricsResponse {
        sessions: SessionMetrics {
            total: session_stats.session_count,
            messages: session_stats.message_count,
        },
        queue: QueueMetrics {
            pending: queue_stats.pending,
            processing: queue_stats.processing,
            completed: queue_stats.completed,
            failed: queue_stats.failed,
            dead: queue_stats.dead,
        },
        runs: RunMetrics {
            running,
            completed,
            failed,
            suspended,
        },
    }))
}

// --- Render helpers ---

fn internal_error(error: anyhow::Error) -> (StatusCode, Html<String>) {
    let page = crate::pages::ErrorPage::internal_error(&error.to_string());
    let html = page
        .render()
        .unwrap_or_else(|_| format!("<p>Internal Server Error: {error}</p>"));
    (StatusCode::INTERNAL_SERVER_ERROR, Html(html))
}

fn render_html<T: Template>(template: &T) -> PartialResult {
    template
        .render()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Html(error.to_string())))
}

fn render_template<T: Template>(template: &T) -> WebResult {
    render_html(template).map(Html)
}

fn render_partial<T: Template>(template: &T) -> PartialResult {
    render_html(template)
}

fn render_dashboard_live_html(db: Arc<Database>) -> PartialResult {
    let dashboard = load_dashboard(db).map_err(internal_error)?;
    render_partial(&DashboardLiveTemplate { dashboard })
}

pub(crate) fn api_error(
    status: StatusCode,
    message: impl std::fmt::Display,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": message.to_string() })),
    )
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

// --- Template structs ---

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
        DashboardView, QueueDetailView, RunDetailView, RunsPageView, ScheduleEditorView,
        SchedulesPageView, SessionDetailView, SessionsPageView, WorkflowDetailView,
        WorkflowsPageView,
    };

    pub(crate) fn render_dashboard_live(dashboard: DashboardView) -> PartialResult {
        render_partial(&DashboardLiveTemplate { dashboard })
    }

    pub(crate) fn render_dashboard_page(
        dashboard: DashboardView,
        live_html: String,
    ) -> PartialResult {
        render_partial(&DashboardTemplate {
            page_title: "Dashboard",
            current_nav: "dashboard",
            dashboard,
            live_html,
        })
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

    pub(crate) fn render_run_detail(detail: RunDetailView) -> PartialResult {
        render_partial(&RunDetailTemplate { detail })
    }

    pub(crate) fn render_runs_page(page: RunsPageView, detail_html: String) -> PartialResult {
        render_partial(&RunsTemplate {
            page_title: "Runs",
            current_nav: "runs",
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
    use std::sync::{Arc, Mutex};

    use opengoose_persistence::{Database, ScheduleStore};
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};

    use super::*;
    use crate::PageState;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home(test: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().expect("env lock should succeed");
        let temp_home = std::env::temp_dir().join(format!(
            "opengoose-routes-schedules-home-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp_home);
        std::fs::create_dir_all(&temp_home).expect("temp home should be created");
        let saved_home = std::env::var("HOME").ok();

        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        test();

        unsafe {
            match saved_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }

        let _ = std::fs::remove_dir_all(temp_home);
    }

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

    #[test]
    fn schedules_handler_renders_existing_schedule() {
        with_temp_home(|| {
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

            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime should build")
                .block_on(async {
                    let Html(html) = schedules(
                        State(PageState { db }),
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
        with_temp_home(|| {
            save_team("ops");
            let db = Arc::new(Database::open_in_memory().expect("db should open"));
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime should build")
                .block_on(async {
                    let Html(html) = schedule_action(
                        State(PageState { db: db.clone() }),
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
}
