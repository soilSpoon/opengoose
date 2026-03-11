use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use opengoose_teams::remote::{ConnectionMetrics, ProtocolMessage};

use super::RemoteGatewayState;

/// DELETE /api/agents/remote/{name} — disconnect a remote agent.
pub async fn disconnect_remote(
    State(state): State<Arc<RemoteGatewayState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let was_connected = state.registry.is_connected(&name).await;
    if was_connected {
        let _ = state
            .registry
            .send_to(
                &name,
                ProtocolMessage::Disconnect {
                    reason: "disconnected by server".into(),
                },
            )
            .await;
        state.registry.unregister(&name).await;
        (StatusCode::OK, format!("disconnected {}", name))
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("agent '{}' not connected", name),
        )
    }
}

/// GET /api/health/gateways — remote agent gateway connection health and metrics.
pub async fn gateway_health(
    State(state): State<Arc<RemoteGatewayState>>,
) -> Json<ConnectionMetrics> {
    Json(state.registry.get_metrics().await)
}
