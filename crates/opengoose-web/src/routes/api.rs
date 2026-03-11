use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use super::health;
use crate::AppState;
use crate::handlers;
use crate::handlers::remote_agents::{self, RemoteGatewayState};
use crate::middleware::{AuthLayer, RateLimitConfig, RateLimitLayer};
use crate::openapi;

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/events", get(handlers::events::stream_events))
        .route(
            "/api/events/history",
            get(handlers::events::list_event_history),
        )
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
        .route("/api/dashboard", get(handlers::dashboard::get_dashboard))
        .route("/api/alerts", get(handlers::alerts::list_alerts))
        .route("/api/alerts", post(handlers::alerts::create_alert))
        .route("/api/alerts/{name}", delete(handlers::alerts::delete_alert))
        .route("/api/alerts/history", get(handlers::alerts::alert_history))
        .route("/api/alerts/test", post(handlers::alerts::test_alerts))
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
            "/api/triggers/{name}/test",
            post(handlers::triggers::test_trigger),
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
        .route(
            "/api/webhooks/{*path}",
            post(handlers::webhooks::receive_webhook),
        )
        .route("/api/health", get(health::health))
        .route("/api/health/ready", get(health::ready))
        .route("/api/health/live", get(health::live))
        .route("/api/metrics", get(health::metrics))
        .route("/api/openapi.json", get(openapi::serve_openapi_json))
        .route("/api/docs", get(openapi::serve_swagger_ui))
        .layer(AuthLayer::new(state.api_key_store.clone()))
        .layer(RateLimitLayer::new(RateLimitConfig::default()))
        .with_state(state)
}

pub(crate) fn remote_router(state: Arc<RemoteGatewayState>) -> Router {
    Router::new()
        .route("/api/agents/connect", get(remote_agents::ws_connect))
        .route("/api/agents/remote", get(remote_agents::list_remote))
        .route(
            "/api/agents/remote/{name}",
            delete(remote_agents::disconnect_remote),
        )
        .route("/api/health/gateways", get(remote_agents::gateway_health))
        .with_state(state)
}
