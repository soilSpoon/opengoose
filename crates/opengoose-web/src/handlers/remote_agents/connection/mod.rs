mod handshake;
mod lifecycle;
mod socket_io;
mod socket_loop;

use std::sync::Arc;

use axum::extract::ws::WebSocket;
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use opengoose_teams::remote::ProtocolMessage;
use tracing::info;

use super::RemoteGatewayState;
use handshake::negotiate_handshake;
use lifecycle::{finalize_connection, register_connection};
use socket_io::send_protocol;
use socket_loop::run_connection_loop;

/// GET /api/agents/connect — WebSocket upgrade for remote agent protocol.
pub async fn ws_connect(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RemoteGatewayState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> impl IntoResponse {
    info!(%addr, "remote agent connection attempt");
    ws.on_upgrade(move |socket| handle_connection(socket, state, addr.to_string()))
}

async fn handle_connection(
    mut socket: WebSocket,
    state: Arc<RemoteGatewayState>,
    endpoint: String,
) {
    let Some(handshake) = negotiate_handshake(&mut socket, &state, &endpoint).await else {
        return;
    };

    let Some((name, mut outbound_rx)) =
        register_connection(&state, &mut socket, &endpoint, handshake).await
    else {
        return;
    };

    info!(%name, %endpoint, "remote agent connected");

    let _ = send_protocol(
        &mut socket,
        &ProtocolMessage::HandshakeAck {
            success: true,
            error: None,
        },
    )
    .await;

    let heartbeat_timeout = state.registry.heartbeat_timeout();
    let disconnect_directive = run_connection_loop(
        &mut socket,
        &state,
        &name,
        &mut outbound_rx,
        heartbeat_timeout,
    )
    .await;

    finalize_connection(state, name, heartbeat_timeout, disconnect_directive).await;
}

#[cfg(test)]
pub(super) async fn reconnect_ack(
    registry: &opengoose_teams::remote::RemoteAgentRegistry,
    name: &str,
    last_event_id: u64,
) -> ProtocolMessage {
    lifecycle::reconnect_ack(registry, name, last_event_id).await
}
