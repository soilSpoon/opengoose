use std::sync::Arc;
use std::time::Duration;

use opengoose_teams::remote::{ProtocolMessage, RemoteAgentRegistry, ReplayResult};
use tokio::sync::mpsc;
use tracing::info;

use super::super::RemoteGatewayState;
use super::handshake::AcceptedHandshake;
use super::socket_io::send_protocol;
use super::socket_loop::ConnectionDirective;

pub(super) async fn register_connection(
    state: &RemoteGatewayState,
    socket: &mut axum::extract::ws::WebSocket,
    endpoint: &str,
    handshake: AcceptedHandshake,
) -> Option<(String, mpsc::UnboundedReceiver<ProtocolMessage>)> {
    let AcceptedHandshake { name, capabilities } = handshake;
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<ProtocolMessage>();

    if let Err(error) = state
        .registry
        .register(name.clone(), capabilities, endpoint.to_owned(), outbound_tx)
        .await
    {
        let _ = send_protocol(
            socket,
            &ProtocolMessage::HandshakeAck {
                success: false,
                error: Some(error),
            },
        )
        .await;
        return None;
    }

    Some((name, outbound_rx))
}

pub(super) async fn finalize_connection(
    state: Arc<RemoteGatewayState>,
    name: String,
    heartbeat_timeout: Duration,
    disconnect_directive: ConnectionDirective,
) {
    match disconnect_directive {
        ConnectionDirective::GracefulDisconnect => {
            state.registry.unregister(&name).await;
            info!(%name, "remote agent unregistered");
        }
        ConnectionDirective::Continue => {
            if state.registry.detach_connection(&name).await {
                let registry = state.registry.clone();
                let reconnect_name = name.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(heartbeat_timeout).await;
                    if registry.unregister_if_detached(&reconnect_name).await {
                        info!(name = %reconnect_name, "remote agent expired after reconnect grace period");
                    }
                });
                info!(%name, "remote agent detached, waiting for reconnect");
            } else {
                state.registry.unregister(&name).await;
                info!(%name, "remote agent unregistered");
            }
        }
    }
}

pub(in super::super) async fn reconnect_ack(
    registry: &RemoteAgentRegistry,
    name: &str,
    last_event_id: u64,
) -> ProtocolMessage {
    registry.mark_reconnecting(name).await;

    let ack = match registry.replay_since(name, last_event_id).await {
        ReplayResult::Replayed(replayed_events) => ProtocolMessage::ReconnectAck {
            success: true,
            replayed_events,
        },
        ReplayResult::BufferMiss | ReplayResult::Unavailable => ProtocolMessage::ReconnectAck {
            success: false,
            replayed_events: 0,
        },
    };

    registry.mark_connected(name).await;
    ack
}
