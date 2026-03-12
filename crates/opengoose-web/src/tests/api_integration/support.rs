use crate::handlers;
use crate::handlers::dashboard::get_dashboard;
use crate::handlers::test_support::make_state;
use crate::routes::health::{health as health_handler, metrics as metrics_handler};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
    routing::{get, post},
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
