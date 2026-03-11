use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use futures_core::Stream;
use opengoose_types::ServiceProbeResponse;

use super::{
    ApiResult, BroadcastLiveOptions, WebResult, api_error, broadcast_live_sse, internal_error,
    render_partial, render_template,
};
use crate::AppState;
use crate::data::{HealthResponse, StatusPageView};
use crate::server::PageState;

mod responses;
mod snapshot;
mod streaming;

pub(crate) use responses::MetricsResponse;

pub(crate) fn page_router(state: PageState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/status/events", get(status_events))
        .with_state(state)
}

pub(crate) async fn status(State(state): State<PageState>) -> WebResult {
    let page = streaming::load_status_page_view(&state).map_err(internal_error)?;
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
    let rx = state.event_bus.subscribe();
    let initial = render_status_stream_html(&state)?;
    let render_state = state.clone();

    Ok(broadcast_live_sse(
        rx,
        initial,
        streaming::matches_status_live_event,
        move || render_status_stream_html(&render_state),
        BroadcastLiveOptions {
            keep_alive_text: "opengoose-status",
            fallback_interval: Some(Duration::from_secs(30)),
            render_on_lagged: true,
            error_html: streaming::status_stream_error_html(),
        },
    ))
}

pub(crate) async fn health(State(state): State<AppState>) -> ApiResult<HealthResponse> {
    let response = responses::build_health_response(state.db, state.channel_metrics)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(response))
}

pub(crate) async fn ready(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<HealthResponse>), (StatusCode, Json<serde_json::Value>)> {
    let (response, ready) = responses::build_ready_response(state.db, state.channel_metrics)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    Ok((status, Json(response)))
}

pub(crate) async fn live() -> Json<ServiceProbeResponse> {
    Json(responses::build_live_response())
}

pub(crate) async fn metrics(State(state): State<AppState>) -> ApiResult<MetricsResponse> {
    let response = responses::build_metrics_response(state.db)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(response))
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
#[template(path = "partials/status_stream.html")]
struct StatusStreamTemplate {
    page: StatusPageView,
}

fn render_status_stream_html(state: &PageState) -> Result<String, (StatusCode, Html<String>)> {
    let page = streaming::load_status_page_view(state).map_err(internal_error)?;
    render_partial(&StatusStreamTemplate { page })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::response::Html;
    use opengoose_persistence::{Database, MessageQueue, MessageType};
    use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
    use opengoose_types::{ChannelMetricsStore, HealthStatus, Platform, SessionKey};

    use super::*;
    use crate::handlers::test_support::make_state;

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

    #[tokio::test]
    async fn metrics_handler_reports_session_queue_and_run_totals() {
        let state = make_state();
        let Json(baseline) = metrics(State(state.clone()))
            .await
            .expect("baseline metrics should succeed");
        let session_key = SessionKey::new(Platform::Discord, "guild", "channel");
        state
            .session_store
            .append_user_message(&session_key, "hello", Some("alice"))
            .expect("user message should be stored");
        state
            .session_store
            .append_assistant_message(&session_key, "hi")
            .expect("assistant message should be stored");

        state
            .orchestration_store
            .create_run("run-1", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("running run should be created");
        state
            .orchestration_store
            .create_run("run-2", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("completed run should be created");
        state
            .orchestration_store
            .complete_run("run-2", "done")
            .expect("run should complete");
        state
            .orchestration_store
            .create_run("run-3", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("failed run should be created");
        state
            .orchestration_store
            .fail_run("run-3", "boom")
            .expect("run should fail");

        MessageQueue::new(Arc::clone(&state.db))
            .enqueue(
                "discord:ns:guild:chan",
                "run-1",
                "user",
                "worker",
                "task",
                MessageType::Task,
            )
            .expect("queue item should be created");

        let Json(response) = metrics(State(state)).await.expect("handler should succeed");

        assert!(response.sessions.total >= baseline.sessions.total);
        assert_eq!(response.sessions.messages, baseline.sessions.messages + 2);
        assert_eq!(response.queue.pending, baseline.queue.pending + 1);
        assert_eq!(response.runs.running, baseline.runs.running + 1);
        assert_eq!(response.runs.completed, baseline.runs.completed + 1);
        assert_eq!(response.runs.failed, baseline.runs.failed + 1);
        assert_eq!(response.runs.suspended, baseline.runs.suspended);
    }
}
