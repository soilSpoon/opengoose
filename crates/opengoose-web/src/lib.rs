/// Dashboard view-model structs and data loaders for the HTML templates.
pub mod data;
/// Typed error types for web handlers with HTTP status code mapping.
pub mod error;
mod handlers;
mod pages;
mod state;

/// Re-exported error type for web API and page handlers.
pub use error::WebError;
/// Re-exported shared application state for all handlers.
pub use state::AppState;
/// Alias kept for backward compatibility.
pub use state::AppState as SharedAppState;

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use askama::Template;
use async_stream::stream;
use axum::Json;
use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use futures_core::Stream;
use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, RunStatus, SessionStore};
use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
use opengoose_types::{AppEventKind, EventBus, SessionKey};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use tracing::{info, warn};

use crate::handlers::remote_agents::{self, RemoteGatewayState};
use crate::pages::not_found_handler;

use crate::data::{
    AgentDetailView, AgentsPageView, DashboardView, QueueDetailView, QueuePageView, RunDetailView,
    RunsPageView, SessionDetailView, SessionsPageView, TeamEditorView, TeamsPageView,
    WorkflowDetailView, WorkflowsPageView, load_agents_page, load_dashboard, load_queue_page,
    load_runs_page, load_sessions_page, load_teams_page, load_workflows_page, save_team_yaml,
};

/// Configuration for the web dashboard server.
#[derive(Debug, Clone, Copy)]
pub struct WebOptions {
    /// Socket address to bind the HTTP listener to.
    pub bind: SocketAddr,
}

impl Default for WebOptions {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from((Ipv4Addr::LOCALHOST, 3000)),
        }
    }
}

#[derive(Clone)]
struct PageState {
    db: Arc<Database>,
}

type WebResult = Result<Html<String>, (StatusCode, Html<String>)>;
type PartialResult = Result<String, (StatusCode, Html<String>)>;

const LIVE_EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct QueueSnapshot {
    last_message_id: Option<i32>,
    last_team_run_id: Option<String>,
    pending: i64,
    processing: i64,
    completed: i64,
    failed: i64,
    dead: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LiveSnapshot {
    sessions: HashMap<String, String>,
    runs: HashMap<String, (String, String)>,
    queue: QueueSnapshot,
}

fn capture_live_snapshot(db: Arc<Database>) -> anyhow::Result<LiveSnapshot> {
    let session_store = SessionStore::new(db.clone());
    let orchestration_store = OrchestrationStore::new(db.clone());
    let queue_store = MessageQueue::new(db);

    let sessions = session_store
        .list_sessions(256)?
        .into_iter()
        .map(|session| (session.session_key, session.updated_at))
        .collect();

    let runs = orchestration_store
        .list_runs(None, 256)?
        .into_iter()
        .map(|run| {
            (
                run.team_run_id,
                (run.updated_at, run.status.as_str().to_string()),
            )
        })
        .collect();

    let queue_stats = queue_store.stats()?;
    let recent_queue = queue_store.list_recent(1)?;
    let queue = QueueSnapshot {
        last_message_id: recent_queue.first().map(|message| message.id),
        last_team_run_id: recent_queue
            .first()
            .map(|message| message.team_run_id.clone()),
        pending: queue_stats.pending,
        processing: queue_stats.processing,
        completed: queue_stats.completed,
        failed: queue_stats.failed,
        dead: queue_stats.dead,
    };

    Ok(LiveSnapshot {
        sessions,
        runs,
        queue,
    })
}

fn emit_live_snapshot_changes(
    previous: &LiveSnapshot,
    current: &LiveSnapshot,
    event_bus: &EventBus,
) {
    let mut dashboard_changed = false;

    for (session_key, updated_at) in &current.sessions {
        if previous.sessions.get(session_key) != Some(updated_at) {
            dashboard_changed = true;
            event_bus.emit(AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id(session_key),
            });
        }
    }
    if previous.sessions.len() != current.sessions.len() {
        dashboard_changed = true;
    }

    for (team_run_id, state) in &current.runs {
        if previous.runs.get(team_run_id) != Some(state) {
            dashboard_changed = true;
            event_bus.emit(AppEventKind::RunUpdated {
                team_run_id: team_run_id.clone(),
                status: state.1.clone(),
            });
        }
    }
    if previous.runs.len() != current.runs.len() {
        dashboard_changed = true;
    }

    if previous.queue != current.queue {
        dashboard_changed = true;
        event_bus.emit(AppEventKind::QueueUpdated {
            team_run_id: current.queue.last_team_run_id.clone(),
        });
    }

    if dashboard_changed {
        event_bus.emit(AppEventKind::DashboardUpdated);
    }
}

fn spawn_live_event_watcher(db: Arc<Database>, event_bus: EventBus) {
    tokio::spawn(async move {
        let mut snapshot = match capture_live_snapshot(db.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!(%error, "failed to capture initial live snapshot");
                LiveSnapshot::default()
            }
        };

        let mut ticker = tokio::time::interval(LIVE_EVENT_POLL_INTERVAL);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match capture_live_snapshot(db.clone()) {
                Ok(next) => {
                    emit_live_snapshot_changes(&snapshot, &next, &event_bus);
                    snapshot = next;
                }
                Err(error) => warn!(%error, "failed to refresh live snapshot"),
            }
        }
    });
}

/// Start the web dashboard and JSON API server.
///
/// Binds to the address in `options`, serves HTML pages, static assets,
/// REST endpoints under `/api/`, and the remote-agent WebSocket gateway.
pub async fn serve(options: WebOptions) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let state = PageState { db: db.clone() };
    let api_state = AppState::new(db)?;
    spawn_live_event_watcher(state.db.clone(), api_state.event_bus.clone());

    let remote_state = Arc::new(RemoteGatewayState {
        registry: RemoteAgentRegistry::new(RemoteConfig::default()),
    });

    let api_routes = Router::new()
        .route("/api/events", get(handlers::events::stream_events))
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route(
            "/api/sessions/{session_key}/messages",
            get(handlers::sessions::get_messages),
        )
        .route("/api/runs", get(handlers::runs::list_runs))
        .route("/api/agents", get(handlers::agents::list_agents))
        .route("/api/teams", get(handlers::teams::list_teams))
        .route("/api/workflows", get(handlers::workflows::list_workflows))
        .route(
            "/api/workflows/{name}",
            get(handlers::workflows::get_workflow),
        )
        .route(
            "/api/workflows/{name}/trigger",
            post(handlers::workflows::trigger_workflow),
        )
        .route("/api/dashboard", get(handlers::dashboard::get_dashboard))
        .route("/api/alerts", get(handlers::alerts::list_alerts))
        .route("/api/alerts", post(handlers::alerts::create_alert))
        .route("/api/alerts/{name}", delete(handlers::alerts::delete_alert))
        .route("/api/alerts/history", get(handlers::alerts::alert_history))
        .route("/api/alerts/test", post(handlers::alerts::test_alerts))
        .route(
            "/api/channel-metrics",
            get(handlers::channel_metrics::get_channel_metrics),
        )
        .route(
            "/api/webhooks/{*path}",
            post(handlers::webhooks::receive_webhook),
        )
        .with_state(api_state);

    // Remote agent API routes (separate state).
    let remote_routes = Router::new()
        .route("/api/agents/connect", get(remote_agents::ws_connect))
        .route("/api/agents/remote", get(remote_agents::list_remote))
        .route(
            "/api/agents/remote/{name}",
            delete(remote_agents::disconnect_remote),
        )
        .with_state(remote_state);

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/dashboard/events", get(dashboard_events))
        .route("/sessions", get(sessions))
        .route("/runs", get(runs))
        .route("/agents", get(agents))
        .route("/workflows", get(workflows))
        .route("/teams", get(teams).post(team_save))
        .route("/queue", get(queue))
        .route("/api/health", get(health))
        .route("/api/metrics", get(metrics))
        .nest_service(
            "/assets",
            ServeDir::new(format!("{}/assets", env!("CARGO_MANIFEST_DIR"))),
        )
        .fallback(not_found_handler)
        .with_state(state)
        .merge(api_routes)
        .merge(remote_routes);

    let listener = tokio::net::TcpListener::bind(options.bind).await?;
    info!(address = %options.bind, "serving opengoose web dashboard");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[derive(Deserialize, Default)]
struct SessionQuery {
    session: Option<String>,
}

#[derive(Deserialize, Default)]
struct RunQuery {
    run: Option<String>,
}

#[derive(Deserialize, Default)]
struct AgentQuery {
    agent: Option<String>,
}

#[derive(Deserialize, Default)]
struct TeamQuery {
    team: Option<String>,
}

#[derive(Deserialize, Default)]
struct WorkflowQuery {
    workflow: Option<String>,
}

#[derive(Deserialize)]
struct TeamSaveForm {
    original_name: String,
    yaml: String,
}

async fn dashboard(State(state): State<PageState>) -> WebResult {
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

async fn dashboard_events(
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

async fn sessions(State(state): State<PageState>, Query(query): Query<SessionQuery>) -> WebResult {
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

async fn runs(State(state): State<PageState>, Query(query): Query<RunQuery>) -> WebResult {
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

async fn agents(Query(query): Query<AgentQuery>) -> WebResult {
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

async fn workflows(
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

async fn teams(Query(query): Query<TeamQuery>) -> WebResult {
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

async fn team_save(Form(form): Form<TeamSaveForm>) -> WebResult {
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

async fn queue(State(state): State<PageState>, Query(query): Query<RunQuery>) -> WebResult {
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
pub fn render_dashboard_live_partial(dashboard: data::DashboardView) -> Result<String, String> {
    DashboardLiveTemplate { dashboard }
        .render()
        .map_err(|e| e.to_string())
}

// --- JSON API types ---

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

fn api_error(
    status: StatusCode,
    message: impl std::fmt::Display,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": message.to_string() })),
    )
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct SessionMetrics {
    total: i64,
    messages: i64,
}

#[derive(Serialize)]
struct QueueMetrics {
    pending: i64,
    processing: i64,
    completed: i64,
    failed: i64,
    dead: i64,
}

#[derive(Serialize)]
struct RunMetrics {
    running: usize,
    completed: usize,
    failed: usize,
    suspended: usize,
}

#[derive(Serialize)]
struct MetricsResponse {
    sessions: SessionMetrics,
    queue: QueueMetrics,
    runs: RunMetrics,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn metrics(State(state): State<PageState>) -> ApiResult<MetricsResponse> {
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
mod tests {
    use super::*;
    use crate::data::{
        ActivityItem, AlertCard, MessageBubble, MetricCard, QueueMessageView, RunListItem,
        SessionListItem, StatusSegment, TrendBar,
    };

    fn sample_dashboard() -> DashboardView {
        DashboardView {
            mode_label: "Live runtime".into(),
            mode_tone: "success",
            stream_summary: "Server-sent events stream fresh snapshots.".into(),
            snapshot_label: "Snapshot 12:34:56 UTC".into(),
            metrics: vec![MetricCard {
                label: "Active runs".into(),
                value: "2".into(),
                note: "1 completed in the latest window".into(),
                tone: "amber",
            }],
            queue_cards: vec![MetricCard {
                label: "Pending".into(),
                value: "4".into(),
                note: "Waiting for pickup".into(),
                tone: "cyan",
            }],
            run_segments: vec![StatusSegment {
                label: "Running".into(),
                value: "2".into(),
                tone: "cyan",
                width: 100,
            }],
            queue_segments: vec![StatusSegment {
                label: "Pending".into(),
                value: "4".into(),
                tone: "amber",
                width: 100,
            }],
            duration_bars: vec![TrendBar {
                label: "feature-dev".into(),
                value: "7m 12s".into(),
                detail: "running".into(),
                tone: "cyan",
                height: 76,
            }],
            activities: vec![ActivityItem {
                actor: "frontend-engineer".into(),
                meta: "Directed to reviewer".into(),
                detail: "Live dashboard shell refreshed over SSE.".into(),
                timestamp: "2026-03-10 12:34".into(),
                tone: "cyan",
            }],
            alerts: vec![AlertCard {
                eyebrow: "Runtime Active".into(),
                title: "2 orchestration runs currently active".into(),
                description: "The dashboard is streaming run status and queue pressure.".into(),
                tone: "success",
            }],
            sessions: vec![SessionListItem {
                title: "ops / bridge".into(),
                subtitle: "feature-dev active · Live runtime".into(),
                preview: "Review the live dashboard state.".into(),
                updated_at: "2026-03-10 12:34".into(),
                badge: "DISCORD".into(),
                badge_tone: "cyan",
                page_url: "/sessions?session=ops".into(),
                active: false,
            }],
            runs: vec![RunListItem {
                title: "feature-dev".into(),
                subtitle: "chain workflow · Live runtime".into(),
                updated_at: "2026-03-10 12:34".into(),
                progress_label: "2/4 steps".into(),
                badge: "RUNNING".into(),
                badge_tone: "cyan",
                page_url: "/runs?run=run-1".into(),
                queue_page_url: "/queue?run=run-1".into(),
                active: false,
            }],
        }
    }

    fn sample_session_detail() -> SessionDetailView {
        SessionDetailView {
            title: "Session ops".into(),
            subtitle: "discord / ops".into(),
            source_label: "Live runtime".into(),
            meta: vec![crate::data::MetaRow {
                label: "Stable key".into(),
                value: "discord:ops:bridge".into(),
            }],
            messages: vec![MessageBubble {
                role_label: "Assistant".into(),
                author_label: "frontend-engineer".into(),
                timestamp: "2026-03-10 12:34".into(),
                content: "Dashboard panel updated.".into(),
                tone: "accent",
                alignment: "right",
            }],
            empty_hint: "No messages yet.".into(),
        }
    }

    fn sample_queue_detail() -> QueueDetailView {
        QueueDetailView {
            title: "Queue run-1".into(),
            subtitle: "feature-dev / chain".into(),
            source_label: "Live runtime".into(),
            status_cards: vec![MetricCard {
                label: "Pending".into(),
                value: "1".into(),
                note: "Waiting for recipients".into(),
                tone: "amber",
            }],
            messages: vec![QueueMessageView {
                sender: "planner".into(),
                recipient: "developer".into(),
                kind: "task".into(),
                status_label: "pending".into(),
                status_tone: "amber",
                created_at: "2026-03-10 12:35".into(),
                retry_text: "1/3".into(),
                content: "Implement the dashboard controls.".into(),
                error: "waiting for pickup".into(),
            }],
            dead_letters: vec![],
            empty_hint: "No queue traffic yet.".into(),
        }
    }

    fn sample_workflow_detail() -> WorkflowDetailView {
        WorkflowDetailView {
            title: "feature-dev".into(),
            subtitle: "Build and verify product changes with a chained team.".into(),
            source_label: "Live registry".into(),
            status_label: "Running".into(),
            status_tone: "cyan",
            meta: vec![crate::data::MetaRow {
                label: "Pattern".into(),
                value: "Chain".into(),
            }],
            steps: vec![crate::data::WorkflowStepView {
                title: "Step 1 · planner".into(),
                detail: "Shape the implementation plan.".into(),
                badge: "Sequential".into(),
                badge_tone: "cyan",
            }],
            automations: vec![crate::data::WorkflowAutomationView {
                kind: "Schedule".into(),
                title: "nightly-review".into(),
                detail: "0 0 * * * · team feature-dev".into(),
                note: "Next 2026-03-11 00:00".into(),
                status_label: "Enabled".into(),
                status_tone: "sage",
            }],
            recent_runs: vec![crate::data::WorkflowRunView {
                title: "Run run-1".into(),
                detail: "2/4 steps · Still executing".into(),
                updated_at: "2026-03-10 12:35".into(),
                status_label: "Running".into(),
                status_tone: "cyan",
                page_url: "/runs?run=run-1".into(),
            }],
            yaml: "title: feature-dev".into(),
            trigger_api_url: "/api/workflows/feature-dev/trigger".into(),
            trigger_input: "Manual run requested".into(),
        }
    }

    #[test]
    fn dashboard_live_template_renders_monitoring_sections() {
        let html = render_partial(&DashboardLiveTemplate {
            dashboard: sample_dashboard(),
        })
        .expect("dashboard live partial renders");

        assert!(html.contains("Execution mix"));
        assert!(html.contains("Queue mix"));
        assert!(html.contains("Agent activity"));
        assert!(html.contains("Live runtime"));
        assert!(html.contains("feature-dev"));
    }

    #[test]
    fn sessions_template_renders_accessible_list_controls() {
        let detail = sample_session_detail();
        let detail_html = render_partial(&SessionDetailTemplate {
            detail: detail.clone(),
        })
        .expect("detail renders");
        let html = render_partial(&SessionsTemplate {
            page_title: "Sessions",
            current_nav: "sessions",
            page: SessionsPageView {
                mode_label: "Live runtime".into(),
                mode_tone: "success",
                sessions: vec![SessionListItem {
                    title: "ops / bridge".into(),
                    subtitle: "feature-dev active · Live runtime".into(),
                    preview: "Investigate the dashboard refresh cycle.".into(),
                    updated_at: "2026-03-10 12:34".into(),
                    badge: "DISCORD".into(),
                    badge_tone: "cyan",
                    page_url: "/sessions?session=ops".into(),
                    active: true,
                }],
                selected: detail,
            },
            detail_html,
        })
        .expect("sessions template renders");

        assert!(html.contains("data-list-shell"));
        assert!(html.contains("Search sessions"));
        assert!(html.contains("data-list-item"));
        assert!(html.contains("data-detail-panel"));
        assert!(!html.contains("hx-get"));
    }

    #[test]
    fn queue_detail_template_renders_table_controls() {
        let html = render_partial(&QueueDetailTemplate {
            detail: sample_queue_detail(),
        })
        .expect("queue detail renders");

        assert!(html.contains("data-table-shell"));
        assert!(html.contains("Search traffic"));
        assert!(html.contains("data-table-row"));
        assert!(html.contains("Retries high-low"));
    }

    #[test]
    fn workflows_template_renders_trigger_controls() {
        let detail = sample_workflow_detail();
        let detail_html = render_partial(&WorkflowDetailTemplate {
            detail: detail.clone(),
        })
        .expect("workflow detail renders");
        let html = render_partial(&WorkflowsTemplate {
            page_title: "Workflows",
            current_nav: "workflows",
            page: WorkflowsPageView {
                mode_label: "Live registry".into(),
                mode_tone: "success",
                workflows: vec![crate::data::WorkflowListItem {
                    title: "feature-dev".into(),
                    subtitle: "Build and verify product changes with a chained team.".into(),
                    preview: "1/1 enabled · 0 configured · planner · developer".into(),
                    source_label: "Live registry".into(),
                    status_label: "Running".into(),
                    status_tone: "cyan",
                    page_url: "/workflows?workflow=feature-dev".into(),
                    active: true,
                }],
                selected: detail,
            },
            detail_html,
        })
        .expect("workflows template renders");

        assert!(html.contains("Search workflows"));
        assert!(html.contains("data-workflow-trigger"));
        assert!(html.contains("/api/workflows/feature-dev/trigger"));
        assert!(html.contains("Recent runs"));
    }

    use crate::handlers;
    use crate::handlers::dashboard::get_dashboard;
    use crate::state::AppState;
    use axum::{
        Json, Router,
        body::Body,
        body::to_bytes,
        extract::State,
        http::{Method, Request, StatusCode, Uri},
        routing::get,
    };
    use opengoose_persistence::{Database, RunStatus};
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn api_metrics(
        State(state): State<AppState>,
    ) -> Result<Json<MetricsResponse>, (StatusCode, Json<serde_json::Value>)> {
        let session_stats = state
            .session_store
            .stats()
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

        let recent_runs = state
            .orchestration_store
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
                pending: 0,
                processing: 0,
                completed: 0,
                failed: 0,
                dead: 0,
            },
            runs: RunMetrics {
                running,
                completed,
                failed,
                suspended,
            },
        }))
    }

    fn api_router() -> Router {
        let state = AppState::new(Arc::new(Database::open_in_memory().unwrap())).unwrap();

        Router::new()
            .route("/api/health", get(health))
            .route("/api/sessions", get(handlers::sessions::list_sessions))
            .route(
                "/api/sessions/{session_key}/messages",
                get(handlers::sessions::get_messages),
            )
            .route("/api/runs", get(handlers::runs::list_runs))
            .route("/api/agents", get(handlers::agents::list_agents))
            .route("/api/teams", get(handlers::teams::list_teams))
            .route("/api/workflows", get(handlers::workflows::list_workflows))
            .route(
                "/api/workflows/{name}",
                get(handlers::workflows::get_workflow),
            )
            .route(
                "/api/workflows/{name}/trigger",
                post(handlers::workflows::trigger_workflow),
            )
            .route("/api/dashboard", get(get_dashboard))
            .route("/api/metrics", get(api_metrics))
            .with_state(state)
    }

    async fn read_json(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&body).expect("response body should be json")
    }

    #[tokio::test]
    async fn api_health_returns_ok() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/health"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = read_json(response).await;
        assert_eq!(payload["status"], "ok");
    }

    #[tokio::test]
    async fn api_dashboard_and_metrics_return_object_payloads() {
        let app = api_router();
        let dashboard = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/dashboard"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("dashboard request should succeed");

        assert_eq!(dashboard.status(), StatusCode::OK);
        let dashboard_body = read_json(dashboard).await;
        assert!(dashboard_body.get("session_count").is_some());

        let metrics = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/metrics"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("metrics request should succeed");

        assert_eq!(metrics.status(), StatusCode::OK);
        let metrics_body = read_json(metrics).await;
        assert!(metrics_body.get("sessions").is_some());
        assert!(metrics_body.get("runs").is_some());
    }

    #[tokio::test]
    async fn api_session_and_run_lists_are_arrays() {
        let app = api_router();

        let sessions = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=10"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("sessions request should succeed");
        assert_eq!(sessions.status(), StatusCode::OK);
        let sessions_body = read_json(sessions).await;
        assert!(sessions_body.is_array());

        let runs = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?limit=10"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("runs request should succeed");
        assert_eq!(runs.status(), StatusCode::OK);
        let runs_body = read_json(runs).await;
        assert!(runs_body.is_array());

        let teams = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/teams"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("teams request should succeed");
        assert_eq!(teams.status(), StatusCode::OK);
        let teams_body = read_json(teams).await;
        assert!(teams_body.is_array());

        let workflows = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/workflows"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("workflows request should succeed");
        assert_eq!(workflows.status(), StatusCode::OK);
        let workflows_body = read_json(workflows).await;
        assert!(workflows_body.is_array());
    }

    #[tokio::test]
    async fn api_session_messages_returns_empty_array_for_missing_session() {
        let app = api_router();
        let messages = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static(
                        "/api/sessions/discord%3Aguild%3Achannel/messages?limit=5",
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("messages request should succeed");

        assert_eq!(messages.status(), StatusCode::OK);
        let body = read_json(messages).await;
        assert!(body.is_array());
    }

    // ── Full API router (includes alerts + fallback) ──────────────────────

    fn full_api_router() -> Router {
        let state = AppState::new(Arc::new(Database::open_in_memory().unwrap())).unwrap();

        Router::new()
            .route("/api/health", get(health))
            .route("/api/sessions", get(handlers::sessions::list_sessions))
            .route(
                "/api/sessions/{session_key}/messages",
                get(handlers::sessions::get_messages),
            )
            .route("/api/runs", get(handlers::runs::list_runs))
            .route("/api/agents", get(handlers::agents::list_agents))
            .route("/api/teams", get(handlers::teams::list_teams))
            .route("/api/workflows", get(handlers::workflows::list_workflows))
            .route(
                "/api/workflows/{name}",
                get(handlers::workflows::get_workflow),
            )
            .route(
                "/api/workflows/{name}/trigger",
                post(handlers::workflows::trigger_workflow),
            )
            .route("/api/dashboard", get(get_dashboard))
            .route("/api/metrics", get(api_metrics))
            .route("/api/alerts", get(handlers::alerts::list_alerts))
            .route(
                "/api/alerts",
                axum::routing::post(handlers::alerts::create_alert),
            )
            .route(
                "/api/alerts/{name}",
                axum::routing::delete(handlers::alerts::delete_alert),
            )
            .route("/api/alerts/history", get(handlers::alerts::alert_history))
            .route(
                "/api/alerts/test",
                axum::routing::post(handlers::alerts::test_alerts),
            )
            .fallback(|| async { StatusCode::NOT_FOUND })
            .with_state(state)
    }

    // ── Alert endpoint integration tests ──────────────────────────────────

    #[tokio::test]
    async fn api_alerts_list_returns_empty_array() {
        let app = full_api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn api_alerts_create_and_list_round_trip() {
        let app = full_api_router();

        // Create an alert rule
        let create_body = serde_json::json!({
            "name": "high-backlog",
            "description": "Queue backlog is too high",
            "metric": "queue_backlog",
            "condition": "gt",
            "threshold": 100.0
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("create request should succeed");

        assert_eq!(create_response.status(), StatusCode::OK);
        let created = read_json(create_response).await;
        assert_eq!(created["name"], "high-backlog");
        assert_eq!(created["metric"], "queue_backlog");
        assert_eq!(created["condition"], "gt");
        assert_eq!(created["threshold"], 100.0);
        assert_eq!(created["enabled"], true);
        assert!(created["id"].is_string());

        // List should now contain the new rule
        let list_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("list request should succeed");

        assert_eq!(list_response.status(), StatusCode::OK);
        let list = read_json(list_response).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "high-backlog");
    }

    #[tokio::test]
    async fn api_alerts_create_rejects_invalid_metric() {
        let app = full_api_router();

        let body = serde_json::json!({
            "name": "bad-metric",
            "metric": "nonexistent_metric",
            "condition": "gt",
            "threshold": 50.0
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let err = read_json(response).await;
        assert!(err["error"].as_str().unwrap().contains("unknown metric"));
    }

    #[tokio::test]
    async fn api_alerts_create_rejects_invalid_condition() {
        let app = full_api_router();

        let body = serde_json::json!({
            "name": "bad-condition",
            "metric": "failed_runs",
            "condition": "neq",
            "threshold": 10.0
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let err = read_json(response).await;
        assert!(err["error"].as_str().unwrap().contains("unknown condition"));
    }

    #[tokio::test]
    async fn api_alerts_delete_nonexistent_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/no-such-alert"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_alerts_history_returns_empty_array() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/alerts/history"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn api_alerts_test_returns_metrics_and_triggered() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts/test"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.get("metrics").is_some());
        assert!(body.get("triggered").is_some());
        assert!(body["triggered"].is_array());
    }

    #[tokio::test]
    async fn api_missing_workflow_trigger_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/workflows/no-such-workflow/trigger"))
                    .header("content-type", "application/json")
                    .body(Body::from(br#"{"input":"run"}"#.to_vec()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ── Fallback / 404 test ───────────────────────────────────────────────

    #[tokio::test]
    async fn api_unknown_route_returns_not_found() {
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/does-not-exist"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ── Sessions handler integration tests ────────────────────────────────

    #[tokio::test]
    async fn api_sessions_list_with_explicit_limit_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=5"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_sessions_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/sessions?limit=abc"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_session_messages_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static(
                        "/api/sessions/discord%3Aguild%3Achannel/messages?limit=notanumber",
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Runs handler integration tests ────────────────────────────────────

    #[tokio::test]
    async fn api_runs_with_running_status_filter_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=running"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_runs_with_completed_status_filter_returns_array() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=completed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::OK);
        let body = read_json(response).await;
        assert!(body.is_array());
    }

    #[tokio::test]
    async fn api_runs_invalid_status_filter_returns_unprocessable() {
        // Invalid status values are rejected by input validation (OPE-67).
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?status=not_a_real_status"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn api_runs_invalid_limit_returns_bad_request() {
        let app = api_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(Uri::from_static("/api/runs?limit=notanumber"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Alerts handler extended integration tests ─────────────────────────

    #[tokio::test]
    async fn api_alerts_create_and_delete_round_trip() {
        let app = full_api_router();

        let create_body = serde_json::json!({
            "name": "delete-me",
            "metric": "failed_runs",
            "condition": "gt",
            "threshold": 5.0
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("create should succeed");

        assert_eq!(create_response.status(), StatusCode::OK);
        let created = read_json(create_response).await;
        assert_eq!(created["name"], "delete-me");

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/delete-me"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("delete should succeed");

        assert_eq!(delete_response.status(), StatusCode::OK);
        let deleted = read_json(delete_response).await;
        assert_eq!(deleted["deleted"], "delete-me");

        // Verify it is gone — a second delete returns 404.
        let gone_response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(Uri::from_static("/api/alerts/delete-me"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("second delete should be handled");

        assert_eq!(gone_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_alerts_create_missing_required_field_returns_unprocessable() {
        let app = full_api_router();

        // `threshold` is missing — JSON body is structurally valid but
        // deserialization will fail, causing Axum to return 422.
        let body = serde_json::json!({
            "name": "incomplete",
            "metric": "failed_runs",
            "condition": "gt"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn api_alerts_create_malformed_json_returns_bad_request() {
        // Syntactically invalid JSON → Axum 0.8 returns 400 Bad Request.
        let app = full_api_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(Uri::from_static("/api/alerts"))
                    .header("content-type", "application/json")
                    .body(Body::from(b"{not valid json}".as_ref()))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
