/// WebSocket gateway for remote agent connections.
///
/// Handles the `/api/agents/connect` WebSocket endpoint that allows
/// remote agents to join OpenGoose teams over the network.
use std::sync::Arc;

use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Serialize;
use tokio::sync::mpsc;
use tracing::{info, warn};

use opengoose_teams::remote::{ProtocolMessage, RemoteAgentRegistry};

/// Shared state for the remote agent gateway.
#[derive(Clone)]
pub struct RemoteGatewayState {
    pub registry: RemoteAgentRegistry,
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

/// GET /api/agents/remote — list currently connected remote agents.
pub async fn list_remote(
    State(state): State<Arc<RemoteGatewayState>>,
) -> Json<Vec<RemoteAgentInfo>> {
    let agents = state.registry.list().await;
    Json(
        agents
            .into_iter()
            .map(|a| RemoteAgentInfo {
                name: a.name,
                capabilities: a.capabilities,
                endpoint: a.endpoint,
                connected_secs: a.connected_at.elapsed().as_secs(),
                last_heartbeat_secs: a.last_heartbeat.elapsed().as_secs(),
            })
            .collect(),
    )
}

/// DELETE /api/agents/remote/{name} — disconnect a remote agent.
pub async fn disconnect_remote(
    State(state): State<Arc<RemoteGatewayState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
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
        (axum::http::StatusCode::OK, format!("disconnected {}", name))
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("agent '{}' not connected", name),
        )
    }
}

/// JSON response describing a currently connected remote agent.
#[derive(Serialize)]
pub struct RemoteAgentInfo {
    /// Registered agent name.
    pub name: String,
    /// Capabilities advertised during handshake.
    pub capabilities: Vec<String>,
    /// Network endpoint the agent connected from.
    pub endpoint: String,
    /// Seconds since the agent connected.
    pub connected_secs: u64,
    /// Seconds since the last heartbeat was received.
    pub last_heartbeat_secs: u64,
}

/// Handle a single WebSocket connection for the remote agent protocol.
async fn handle_connection(
    mut socket: WebSocket,
    state: Arc<RemoteGatewayState>,
    endpoint: String,
) {
    // Wait for handshake as the first message.
    let handshake_msg = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        recv_text(&mut socket),
    )
    .await
    {
        Ok(Some(text)) => text,
        Ok(None) => {
            warn!(%endpoint, "connection closed before handshake");
            return;
        }
        Err(_) => {
            warn!(%endpoint, "handshake timeout");
            let _ = send_protocol(
                &mut socket,
                &ProtocolMessage::Error {
                    message: "handshake timeout".into(),
                },
            )
            .await;
            return;
        }
    };

    // Parse and validate handshake.
    let (name, capabilities) = match serde_json::from_str::<ProtocolMessage>(&handshake_msg) {
        Ok(ProtocolMessage::Handshake {
            agent_name: name,
            api_key,
            capabilities,
        }) => {
            if !state.registry.validate_key(&api_key) {
                let _ = send_protocol(
                    &mut socket,
                    &ProtocolMessage::HandshakeAck {
                        success: false,
                        error: Some("invalid api key".into()),
                    },
                )
                .await;
                return;
            }
            (name, capabilities)
        }
        _ => {
            let _ = send_protocol(
                &mut socket,
                &ProtocolMessage::Error {
                    message: "expected handshake message".into(),
                },
            )
            .await;
            return;
        }
    };

    // Register outbound channel for sending messages to this agent.
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<ProtocolMessage>();

    if let Err(e) = state
        .registry
        .register(name.clone(), capabilities, endpoint.clone(), outbound_tx)
        .await
    {
        let _ = send_protocol(
            &mut socket,
            &ProtocolMessage::HandshakeAck {
                success: false,
                error: Some(e),
            },
        )
        .await;
        return;
    }

    info!(%name, %endpoint, "remote agent connected");

    let _ = send_protocol(
        &mut socket,
        &ProtocolMessage::HandshakeAck {
            success: true,
            error: None,
        },
    )
    .await;

    // Main message loop: multiplex incoming WebSocket messages and outbound queue.
    let heartbeat_interval = state.registry.heartbeat_interval();
    let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
    heartbeat_timer.tick().await; // consume immediate tick

    loop {
        tokio::select! {
            // Incoming message from remote agent.
            msg = recv_text(&mut socket) => {
                match msg {
                    Some(text) => {
                        if !handle_incoming(&state, &name, &text, &mut socket).await {
                            break;
                        }
                    }
                    None => {
                        info!(%name, "remote agent connection closed");
                        break;
                    }
                }
            }
            // Outbound message to send to remote agent.
            msg = outbound_rx.recv() => {
                match msg {
                    Some(protocol_msg) => {
                        if send_protocol(&mut socket, &protocol_msg).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Periodic heartbeat from server.
            _ = heartbeat_timer.tick() => {
                let hb = ProtocolMessage::Heartbeat {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                };
                if send_protocol(&mut socket, &hb).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup on disconnect.
    state.registry.unregister(&name).await;
    info!(%name, "remote agent unregistered");
}

/// Handle an incoming protocol message. Returns false if the connection should close.
async fn handle_incoming(
    state: &RemoteGatewayState,
    name: &str,
    text: &str,
    socket: &mut WebSocket,
) -> bool {
    match serde_json::from_str::<ProtocolMessage>(text) {
        Ok(ProtocolMessage::Heartbeat { .. }) => {
            state.registry.touch_heartbeat(name).await;
            true
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
            true
        }
        Ok(ProtocolMessage::Disconnect { reason }) => {
            info!(%name, %reason, "remote agent disconnecting");
            false
        }
        Ok(_) => true, // ignore unexpected message types
        Err(e) => {
            warn!(%name, error = %e, "invalid protocol message");
            true
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
            Some(Ok(_)) => continue, // skip ping/pong/binary
            Some(Err(e)) => {
                let err_lower = e.to_string().to_lowercase();
                if err_lower.contains("tls")
                    || err_lower.contains("certificate")
                    || err_lower.contains("handshake")
                {
                    warn!(error = %e, "TLS handshake error on remote agent connection");
                } else {
                    warn!(error = %e, "websocket receive error");
                }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};

    use super::{RemoteGatewayState, disconnect_remote, list_remote};

    fn make_state(config: RemoteConfig) -> Arc<RemoteGatewayState> {
        Arc::new(RemoteGatewayState {
            registry: RemoteAgentRegistry::new(config),
        })
    }

    #[tokio::test]
    async fn list_remote_empty_registry() {
        let state = make_state(RemoteConfig::default());
        let axum::Json(agents) = list_remote(State(state)).await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn list_remote_with_registered_agents() {
        let state = make_state(RemoteConfig::default());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register(
                "remote-a".into(),
                vec!["cap-x".into()],
                "ws://remote-a:9000".into(),
                tx,
            )
            .await
            .unwrap();

        let axum::Json(agents) = list_remote(State(state)).await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "remote-a");
        assert_eq!(agents[0].capabilities, vec!["cap-x"]);
        assert_eq!(agents[0].endpoint, "ws://remote-a:9000");
    }

    #[tokio::test]
    async fn list_remote_reflects_multiple_agents() {
        let state = make_state(RemoteConfig::default());
        for i in 0..3 {
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            state
                .registry
                .register(
                    format!("agent-{i}"),
                    vec![],
                    format!("ws://host:{}", 9000 + i),
                    tx,
                )
                .await
                .unwrap();
        }

        let axum::Json(agents) = list_remote(State(state)).await;
        assert_eq!(agents.len(), 3);
    }

    #[tokio::test]
    async fn disconnect_remote_connected_agent_returns_ok() {
        let state = make_state(RemoteConfig::default());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("conn-agent".into(), vec![], "ws://c".into(), tx)
            .await
            .unwrap();

        let response = disconnect_remote(State(state.clone()), Path("conn-agent".into()))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!state.registry.is_connected("conn-agent").await);
    }

    #[tokio::test]
    async fn disconnect_remote_unknown_agent_returns_not_found() {
        let state = make_state(RemoteConfig::default());
        let response = disconnect_remote(State(state), Path("ghost".into()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn disconnect_remote_sends_disconnect_message_to_agent() {
        use opengoose_teams::remote::ProtocolMessage;

        let state = make_state(RemoteConfig::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("msg-agent".into(), vec![], "ws://m".into(), tx)
            .await
            .unwrap();

        let _ = disconnect_remote(State(state), Path("msg-agent".into())).await;

        // The handler sends a Disconnect message before unregistering.
        let msg = rx.try_recv().expect("disconnect message should be queued");
        match msg {
            ProtocolMessage::Disconnect { reason } => {
                assert!(reason.contains("server"));
            }
            other => panic!("expected Disconnect, got {:?}", other),
        }
    }
}
