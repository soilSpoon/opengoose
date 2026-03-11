use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use chrono::{SecondsFormat, Utc};
use futures_core::Stream;
use opengoose_persistence::{MessageQueue, OrchestrationStore, RunStatus, SessionStore};
use opengoose_types::{HealthStatus, ServiceProbeResponse};

use super::{
    ApiResult, WebResult, api_error, datastar_patch_elements_event, internal_error, render_partial,
    render_template,
};
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

pub(crate) async fn status_events(
    State(state): State<PageState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let initial = render_status_stream_html(&state)?;
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match render_status_stream_html(&state) {
                Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                Err(_) => yield Ok(datastar_patch_elements_event(status_stream_error_html())),
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-status"),
    ))
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

    let mut running = 0;
    let mut completed = 0;
    let mut failed = 0;
    let mut suspended = 0;
    for run in &recent_runs {
        match run.status {
            RunStatus::Running => running += 1,
            RunStatus::Completed => completed += 1,
            RunStatus::Failed => failed += 1,
            RunStatus::Suspended => suspended += 1,
        }
    }

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

#[derive(Template)]
#[template(path = "partials/status_page_intro.html")]
struct StatusPageIntroTemplate {
    page: StatusPageView,
}

fn render_status_stream_html(state: &PageState) -> Result<String, (StatusCode, Html<String>)> {
    let page = load_status_page(state.db.clone(), state.channel_metrics.clone())
        .map_err(internal_error)?;
    let intro_html = render_partial(&StatusPageIntroTemplate { page: page.clone() })?;
    let live_html = render_partial(&StatusLiveTemplate { page })?;
    Ok(format!(
        r#"{intro}<div id="status-live">{live}</div>"#,
        intro = intro_html,
        live = live_html
    ))
}

fn status_stream_error_html() -> &'static str {
    r#"
<section id="status-page-intro" class="hero-panel">
  <div class="hero-copy">
    <p class="eyebrow">System status</p>
    <h1>Status snapshot unavailable.</h1>
    <p class="hero-text">The health board is retrying in the background.</p>
  </div>
  <div class="hero-status">
    <p class="eyebrow">Refresh loop</p>
    <div class="live-chip-row">
      <span class="chip tone-rose">Stream degraded</span>
    </div>
    <p>Retrying automatically.</p>
  </div>
</section>
<div id="status-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Status unavailable</p>
    <h2>Live health probe failed</h2>
    <p>The board will retry on the next refresh interval.</p>
  </section>
</div>
"#
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
        assert!(html.contains(
            "data-init=\"@get('/status/events', { openWhenHidden: true, retry: 'always' })\""
        ));
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
