use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use chrono::{SecondsFormat, Utc};
use futures_core::Stream;
use opengoose_persistence::{
    DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES, MessageQueue, RunStatus, SessionMetricItem,
};
use opengoose_types::{HealthStatus, ServiceProbeResponse};

use super::{ApiResult, WebResult, api_error, internal_error, render_partial, render_template};
use crate::AppState;
use crate::data::{
    HealthResponse, StatusPageView, load_status_page, probe_health, probe_readiness,
};
use crate::server::PageState;

#[derive(serde::Serialize)]
pub(crate) struct SessionMetrics {
    pub(crate) total: i64,
    pub(crate) messages: i64,
    pub(crate) estimated_tokens: i64,
    pub(crate) active: i64,
    pub(crate) active_window_minutes: i64,
    pub(crate) average_duration_seconds: f64,
    pub(crate) per_session: Vec<SessionMetricsItem>,
}

#[derive(serde::Serialize)]
pub(crate) struct SessionMetricsItem {
    pub(crate) session_key: String,
    pub(crate) active_team: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) message_count: i64,
    pub(crate) estimated_tokens: i64,
    pub(crate) duration_seconds: i64,
    pub(crate) active: bool,
}

#[derive(serde::Serialize)]
pub(crate) struct QueueMetrics {
    pub(crate) pending: i64,
    pub(crate) processing: i64,
    pub(crate) completed: i64,
    pub(crate) failed: i64,
    pub(crate) dead: i64,
}

#[derive(serde::Serialize)]
pub(crate) struct RunMetrics {
    pub(crate) running: usize,
    pub(crate) completed: usize,
    pub(crate) failed: usize,
    pub(crate) suspended: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct MetricsResponse {
    pub(crate) sessions: SessionMetrics,
    pub(crate) queue: QueueMetrics,
    pub(crate) runs: RunMetrics,
}

pub(crate) fn page_router(state: PageState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/status/events", get(status_events))
        .with_state(state)
}

pub(crate) async fn status(State(state): State<PageState>) -> WebResult {
    let page = load_status_page(state.db, state.channel_metrics).map_err(internal_error)?;
    let live_html = render_partial(&StatusLiveTemplate { page: page.clone() })?;

    render_template(&StatusTemplate {
        page_title: "System Status",
        current_nav: "status",
        page,
        live_html,
    })
}

pub(crate) async fn status_events() -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send> {
    let event_stream = stream! {
        yield Ok(Event::default().data("status-ready"));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            yield Ok(Event::default().data("status-refresh"));
        }
    };

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-status"),
    )
}

pub(crate) async fn health(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    let response = probe_health(state.db, state.channel_metrics)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(response))
}

pub(crate) async fn ready(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<HealthResponse>), (StatusCode, Json<serde_json::Value>)> {
    let (response, ready) = probe_readiness(state.db, state.channel_metrics)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    Ok((status, Json(response)))
}

pub(crate) async fn live() -> Json<ServiceProbeResponse> {
    Json(ServiceProbeResponse {
        status: HealthStatus::Healthy,
        checked_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

pub(crate) async fn metrics(State(state): State<AppState>) -> ApiResult<MetricsResponse> {
    let session_stats = state
        .session_store
        .stats()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let session_breakdown = state
        .session_store
        .list_session_metrics(100)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let queue_stats = MessageQueue::new(state.db.clone())
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
            estimated_tokens: session_stats.estimated_token_count,
            active: session_stats.active_session_count,
            active_window_minutes: DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES,
            average_duration_seconds: session_stats.average_session_duration_seconds,
            per_session: session_breakdown
                .into_iter()
                .map(session_metrics_item)
                .collect(),
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

fn session_metrics_item(item: SessionMetricItem) -> SessionMetricsItem {
    SessionMetricsItem {
        session_key: item.session_key,
        active_team: item.active_team,
        created_at: item.created_at,
        updated_at: item.updated_at,
        message_count: item.message_count,
        estimated_tokens: item.estimated_token_count,
        duration_seconds: item.duration_seconds,
        active: item.active,
    }
}

#[derive(Template)]
#[template(path = "status.html")]
struct StatusTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: StatusPageView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/status_live.html")]
struct StatusLiveTemplate {
    page: StatusPageView,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::response::Html;
    use opengoose_persistence::Database;
    use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
    use opengoose_types::ChannelMetricsStore;

    use super::*;

    fn page_state(db: Arc<Database>) -> PageState {
        PageState {
            db,
            remote_registry: RemoteAgentRegistry::new(RemoteConfig::default()),
            channel_metrics: ChannelMetricsStore::new(),
            event_bus: opengoose_types::EventBus::new(256),
        }
    }

    #[tokio::test]
    async fn status_handler_renders_component_and_gateway_health() {
        let state = page_state(Arc::new(
            Database::open_in_memory().expect("db should open"),
        ));
        state.channel_metrics.set_connected("discord");
        state
            .channel_metrics
            .record_reconnect("slack", Some("timeout".into()));

        let Html(html) = status(State(state)).await.expect("handler should render");

        assert!(html.contains("/assets/styles/shared.css"));
        assert!(html.contains("/assets/styles/monitoring.css"));
        assert!(html.contains("System status"));
        assert!(html.contains("data-live-events-url=\"/status/events\""));
        assert!(html.contains("Gateway connections"));
        assert!(html.contains("discord"));
        assert!(html.contains("slack"));
    }

    #[tokio::test]
    async fn live_handler_returns_healthy() {
        let Json(response) = live().await;

        assert_eq!(response.status, HealthStatus::Healthy);
        assert!(response.checked_at.ends_with('Z'));
    }

    #[tokio::test]
    async fn metrics_handler_reports_session_breakdown() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        let state = AppState::new(db.clone()).expect("app state should build");
        let key = opengoose_types::SessionKey::from_stable_id("discord:ns:ops:bridge");

        state
            .session_store
            .append_user_message(&key, "12345678", Some("alice"))
            .expect("user message should persist");
        std::thread::sleep(Duration::from_secs(1));
        state
            .session_store
            .append_assistant_message(&key, "1234")
            .expect("assistant message should persist");

        let Json(response) = metrics(State(state))
            .await
            .expect("metrics handler should succeed");

        assert_eq!(response.sessions.total, 1);
        assert_eq!(response.sessions.messages, 2);
        assert_eq!(response.sessions.estimated_tokens, 3);
        assert_eq!(response.sessions.active, 1);
        assert_eq!(
            response.sessions.active_window_minutes,
            DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES
        );
        assert!(response.sessions.average_duration_seconds >= 1.0);
        assert_eq!(response.sessions.per_session.len(), 1);
        assert_eq!(
            response.sessions.per_session[0].session_key,
            "discord:ns:ops:bridge"
        );
        assert_eq!(response.sessions.per_session[0].message_count, 2);
        assert_eq!(response.sessions.per_session[0].estimated_tokens, 3);
        assert!(response.sessions.per_session[0].active);
    }
}
