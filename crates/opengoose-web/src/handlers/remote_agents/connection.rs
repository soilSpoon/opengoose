use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use opengoose_teams::remote::{ProtocolMessage, RemoteAgentRegistry, ReplayResult};
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::RemoteGatewayState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionDirective {
    Continue,
    GracefulDisconnect,
}

struct AcceptedHandshake {
    name: String,
    capabilities: Vec<String>,
}

/// GET /api/agents/connect — WebSocket upgrade for remote agent protocol.
pub async fn ws_connect(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RemoteGatewayState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> impl IntoResponse {
    info!(%addr, "remote agent connection attempt");
    ws.on_upgrade(move |socket| handle_connection(socket, state, addr.to_string()))
}

/// Handle a single WebSocket connection for the remote agent protocol.
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

async fn negotiate_handshake(
    socket: &mut WebSocket,
    state: &RemoteGatewayState,
    endpoint: &str,
) -> Option<AcceptedHandshake> {
    let handshake_msg = match tokio::time::timeout(Duration::from_secs(10), recv_text(socket)).await
    {
        Ok(Some(text)) => text,
        Ok(None) => {
            warn!(%endpoint, "connection closed before handshake");
            return None;
        }
        Err(_) => {
            warn!(%endpoint, "handshake timeout");
            let _ = send_protocol(
                socket,
                &ProtocolMessage::Error {
                    message: "handshake timeout".into(),
                },
            )
            .await;
            return None;
        }
    };

    parse_handshake(state, socket, &handshake_msg).await
}

async fn parse_handshake(
    state: &RemoteGatewayState,
    socket: &mut WebSocket,
    handshake_msg: &str,
) -> Option<AcceptedHandshake> {
    match serde_json::from_str::<ProtocolMessage>(handshake_msg) {
        Ok(ProtocolMessage::Handshake {
            agent_name: name,
            api_key,
            capabilities,
        }) => {
            if !state.registry.validate_key(&api_key) {
                let _ = send_protocol(
                    socket,
                    &ProtocolMessage::HandshakeAck {
                        success: false,
                        error: Some("invalid api key".into()),
                    },
                )
                .await;
                return None;
            }

            Some(AcceptedHandshake { name, capabilities })
        }
        _ => {
            let _ = send_protocol(
                socket,
                &ProtocolMessage::Error {
                    message: "expected handshake message".into(),
                },
            )
            .await;
            None
        }
    }
}

async fn register_connection(
    state: &RemoteGatewayState,
    socket: &mut WebSocket,
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

async fn run_connection_loop(
    socket: &mut WebSocket,
    state: &RemoteGatewayState,
    name: &str,
    outbound_rx: &mut mpsc::UnboundedReceiver<ProtocolMessage>,
    heartbeat_timeout: Duration,
) -> ConnectionDirective {
    let mut heartbeat_timer = tokio::time::interval(state.registry.heartbeat_interval());
    heartbeat_timer.tick().await; // consume immediate tick

    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match handle_incoming(state, name, &text, socket).await {
                            ConnectionDirective::Continue => {}
                            ConnectionDirective::GracefulDisconnect => {
                                return ConnectionDirective::GracefulDisconnect;
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = Instant::now();
                        state.registry.touch_heartbeat(name).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(%name, "remote agent connection closed");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        log_socket_error(Some(name), &error);
                        break;
                    }
                }
            }
            msg = outbound_rx.recv() => {
                match msg {
                    Some(protocol_msg) => {
                        if send_protocol(socket, &protocol_msg).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = heartbeat_timer.tick() => {
                if last_pong.elapsed() > heartbeat_timeout {
                    warn!(%name, "heartbeat timeout, disconnecting stale remote agent");
                    let _ = send_protocol(
                        socket,
                        &ProtocolMessage::Error {
                            message: "heartbeat timeout".into(),
                        },
                    )
                    .await;
                    break;
                }

                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }

    ConnectionDirective::Continue
}

async fn finalize_connection(
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

/// Handle an incoming protocol message.
async fn handle_incoming(
    state: &RemoteGatewayState,
    name: &str,
    text: &str,
    socket: &mut WebSocket,
) -> ConnectionDirective {
    match serde_json::from_str::<ProtocolMessage>(text) {
        Ok(ProtocolMessage::Heartbeat { .. }) => {
            state.registry.touch_heartbeat(name).await;
            ConnectionDirective::Continue
        }
        Ok(ProtocolMessage::MessageRelay { from, to, payload }) => {
            let relay = ProtocolMessage::MessageRelay {
                from,
                to: to.clone(),
                payload,
            };
            if !state.registry.send_to(&to, relay).await {
                let _ = send_protocol(
                    socket,
                    &ProtocolMessage::Error {
                        message: format!("agent '{}' not connected", to),
                    },
                )
                .await;
            }
            ConnectionDirective::Continue
        }
        Ok(ProtocolMessage::Reconnect { last_event_id }) => {
            info!(%name, %last_event_id, "remote agent reconnecting");
            let ack = reconnect_ack(&state.registry, name, last_event_id).await;
            let _ = send_protocol(socket, &ack).await;
            ConnectionDirective::Continue
        }
        Ok(ProtocolMessage::Disconnect { reason }) => {
            info!(%name, %reason, "remote agent disconnecting");
            ConnectionDirective::GracefulDisconnect
        }
        Ok(_) => ConnectionDirective::Continue,
        Err(error) => {
            warn!(%name, error = %error, "invalid protocol message");
            ConnectionDirective::Continue
        }
    }
}

/// Receive the next text message from the WebSocket, skipping pings/pongs.
///
/// WebSocket errors — including TLS handshake failures that surface after the
/// HTTP upgrade — are logged and treated as a clean connection close so the
/// registry is not left with stale entries.
async fn recv_text(socket: &mut WebSocket) -> Option<String> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => return Some(text.to_string()),
            Some(Ok(Message::Close(_))) | None => return None,
            Some(Ok(_)) => continue,
            Some(Err(error)) => {
                log_socket_error(None, &error);
                return None;
            }
        }
    }
}

/// Send a protocol message as JSON text over the WebSocket.
async fn send_protocol(socket: &mut WebSocket, msg: &ProtocolMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(msg).map_err(axum::Error::new)?;
    socket.send(Message::Text(json.into())).await
}

pub(super) async fn reconnect_ack(
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

fn log_socket_error(name: Option<&str>, error: &axum::Error) {
    let error_text = error.to_string();
    let error_lower = error_text.to_lowercase();
    if error_lower.contains("tls")
        || error_lower.contains("certificate")
        || error_lower.contains("handshake")
    {
        warn!(error = %error_text, "TLS handshake error on remote agent connection");
    } else if let Some(name) = name {
        warn!(%name, error = %error_text, "websocket receive error");
    } else {
        warn!(error = %error_text, "websocket receive error");
    }
}
