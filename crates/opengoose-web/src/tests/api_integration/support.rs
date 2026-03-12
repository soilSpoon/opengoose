use crate::handlers;
use crate::handlers::dashboard::get_dashboard;
use crate::handlers::test_support::make_state;
use crate::routes::health::{
    health as health_handler, live as live_handler, metrics as metrics_handler,
    ready as ready_handler,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
    routing::{delete, get, patch, post, put},
};
use serde_json::Value;

pub(super) fn api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
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
        .route("/api/metrics", get(metrics_handler))
        .with_state(state)
}

pub(super) fn full_api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
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
        .route("/api/metrics", get(metrics_handler))
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

/// Router with all API routes including events, triggers, channel-metrics, and gateways.
pub(super) fn complete_api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/health/ready", get(ready_handler))
        .route("/api/health/live", get(live_handler))
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
        .route("/api/metrics", get(metrics_handler))
        .route("/api/alerts", get(handlers::alerts::list_alerts))
        .route("/api/alerts", post(handlers::alerts::create_alert))
        .route("/api/alerts/{name}", delete(handlers::alerts::delete_alert))
        .route("/api/alerts/history", get(handlers::alerts::alert_history))
        .route("/api/alerts/test", post(handlers::alerts::test_alerts))
        .route(
            "/api/events/history",
            get(handlers::events::list_event_history),
        )
        .route("/api/triggers", get(handlers::triggers::list_triggers))
        .route("/api/triggers", post(handlers::triggers::create_trigger))
        .route("/api/triggers/{name}", get(handlers::triggers::get_trigger))
        .route(
            "/api/triggers/{name}",
            put(handlers::triggers::update_trigger),
        )
        .route(
            "/api/triggers/{name}",
            delete(handlers::triggers::delete_trigger),
        )
        .route(
            "/api/triggers/{name}/enabled",
            patch(handlers::triggers::set_trigger_enabled),
        )
        .route(
            "/api/channel-metrics",
            get(handlers::channel_metrics::get_channel_metrics),
        )
        .route("/api/gateways", get(handlers::gateways::list_gateways))
        .route(
            "/api/gateways/{platform}/status",
            get(handlers::gateways::gateway_status),
        )
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state)
}

fn request(method: Method, path: &str, content_type: Option<&str>, body: Body) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);

    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }

    builder.body(body).unwrap()
}

pub(super) fn empty_request(method: Method, path: &str) -> Request<Body> {
    request(method, path, None, Body::empty())
}

pub(super) fn json_request(method: Method, path: &str, body: Value) -> Request<Body> {
    request(
        method,
        path,
        Some("application/json"),
        Body::from(serde_json::to_vec(&body).unwrap()),
    )
}

pub(super) fn raw_json_request<T>(method: Method, path: &str, body: T) -> Request<Body>
where
    T: Into<Body>,
{
    request(method, path, Some("application/json"), body.into())
}

pub(super) async fn read_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be json")
}
