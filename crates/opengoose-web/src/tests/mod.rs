//! Test infrastructure shared across web-crate integration tests.
//!
//! Provides two pre-wired Axum routers (`api_router` / `full_api_router`),
//! a JSON response reader, and an in-test metrics handler.

mod api_alerts;
mod api_core;
mod config;
mod templates;

use crate::handlers;
use crate::handlers::dashboard::get_dashboard;
use crate::handlers::test_support::make_state;
use crate::routes;
use crate::routes::health::{
    MetricsResponse, QueueMetrics, RunMetrics, SessionMetrics, health as health_handler,
    live as live_handler, ready as ready_handler,
};
use crate::state::AppState;
use axum::{
    Json, Router,
    body::to_bytes,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use opengoose_persistence::RunStatus;
use serde_json::Value;

async fn api_metrics(
    State(state): State<AppState>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let session_stats = state
        .session_store
        .stats()
        .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let recent_runs = state
        .orchestration_store
        .list_runs(None, 200)
        .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

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
            estimated_tokens: 0,
            active: 0,
            active_window_minutes: 15,
            average_duration_seconds: 0.0,
            per_session: vec![],
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
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/health/ready", get(ready_handler))
        .route("/api/health/live", get(live_handler))
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route(
            "/api/sessions/export",
            get(handlers::sessions::export_sessions),
        )
        .route(
            "/api/sessions/{session_key}/messages",
            get(handlers::sessions::get_messages),
        )
        .route(
            "/api/sessions/{session_key}/export",
            get(handlers::sessions::export_session),
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

fn full_api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/health/ready", get(ready_handler))
        .route("/api/health/live", get(live_handler))
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route(
            "/api/sessions/export",
            get(handlers::sessions::export_sessions),
        )
        .route(
            "/api/sessions/{session_key}/messages",
            get(handlers::sessions::get_messages),
        )
        .route(
            "/api/sessions/{session_key}/export",
            get(handlers::sessions::export_session),
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

async fn read_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be json")
}
