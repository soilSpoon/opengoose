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
use opengoose_persistence::{MessageQueue, OrchestrationStore, RunStatus, SessionStore};
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
}
