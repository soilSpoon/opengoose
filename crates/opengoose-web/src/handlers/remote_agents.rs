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
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{info, warn};

use opengoose_teams::remote::{
    ConnectionMetrics, ProtocolMessage, RemoteAgentRegistry, ReplayResult,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionDirective {
    Continue,
    GracefulDisconnect,
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
    let heartbeat_timeout = state.registry.heartbeat_timeout();
    let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
    heartbeat_timer.tick().await; // consume immediate tick

    // Track when we last received a pong (or first connected).
    let mut last_pong = Instant::now();
    let mut disconnect_directive = ConnectionDirective::Continue;

    loop {
        tokio::select! {
            // Incoming message from remote agent.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match handle_incoming(&state, &name, &text, &mut socket).await {
                            ConnectionDirective::Continue => {}
                            ConnectionDirective::GracefulDisconnect => {
                                disconnect_directive = ConnectionDirective::GracefulDisconnect;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to our ping — reset the timeout clock.
                        last_pong = Instant::now();
                        state.registry.touch_heartbeat(&name).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(%name, "remote agent connection closed");
                        break;
                    }
                    Some(Ok(_)) => {} // skip ping/binary
                    Some(Err(e)) => {
                        let err_lower = e.to_string().to_lowercase();
                        if err_lower.contains("tls")
                            || err_lower.contains("certificate")
                            || err_lower.contains("handshake")
                        {
                            warn!(error = %e, "TLS handshake error on remote agent connection");
                        } else {
                            warn!(%name, error = %e, "websocket receive error");
                        }
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
            // Periodic heartbeat: check for timeout, then send a WS ping.
            _ = heartbeat_timer.tick() => {
                if last_pong.elapsed() > heartbeat_timeout {
                    warn!(%name, "heartbeat timeout, disconnecting stale remote agent");
                    let _ = send_protocol(
                        &mut socket,
                        &ProtocolMessage::Error {
                            message: "heartbeat timeout".into(),
                        },
                    )
                    .await;
                    break;
                }
                // Send a WebSocket-level ping; the client MUST reply with a pong.
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }

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
        Ok(_) => ConnectionDirective::Continue, // ignore unexpected message types
        Err(e) => {
            warn!(%name, error = %e, "invalid protocol message");
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

async fn reconnect_ack(
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

/// GET /api/health/gateways — remote agent gateway connection health and metrics.
pub async fn gateway_health(
    State(state): State<std::sync::Arc<RemoteGatewayState>>,
) -> Json<ConnectionMetrics> {
    Json(state.registry.get_metrics().await)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use opengoose_teams::remote::{ProtocolMessage, RemoteAgentRegistry, RemoteConfig};

    use super::{RemoteGatewayState, disconnect_remote, list_remote, reconnect_ack};

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

    #[tokio::test]
    async fn disconnect_remote_updates_gateway_metrics() {
        use super::gateway_health;

        let state = make_state(RemoteConfig::default());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("metrics-agent".into(), vec![], "ws://metrics".into(), tx)
            .await
            .unwrap();

        let response = disconnect_remote(State(state.clone()), Path("metrics-agent".into()))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);

        let axum::Json(metrics) = gateway_health(State(state)).await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 1);
    }

    #[tokio::test]
    async fn reconnect_ack_reports_replayed_event_count() {
        let state = make_state(RemoteConfig {
            replay_buffer_capacity: 4,
            ..RemoteConfig::default()
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("replay-agent".into(), vec![], "ws://replay".into(), tx)
            .await
            .unwrap();

        assert!(
            state
                .registry
                .send_to(
                    "replay-agent",
                    ProtocolMessage::MessageRelay {
                        from: "local".into(),
                        to: "replay-agent".into(),
                        payload: "first".into(),
                    },
                )
                .await
        );
        assert!(
            state
                .registry
                .send_to(
                    "replay-agent",
                    ProtocolMessage::MessageRelay {
                        from: "local".into(),
                        to: "replay-agent".into(),
                        payload: "second".into(),
                    },
                )
                .await
        );

        let _ = rx
            .recv()
            .await
            .expect("initial first delivery should exist");
        let _ = rx
            .recv()
            .await
            .expect("initial second delivery should exist");

        match reconnect_ack(&state.registry, "replay-agent", 1).await {
            ProtocolMessage::ReconnectAck {
                success,
                replayed_events,
            } => {
                assert!(success);
                assert_eq!(replayed_events, 1);
            }
            other => panic!("expected reconnect ack, got {other:?}"),
        }

        match rx.recv().await.expect("replayed delivery should exist") {
            ProtocolMessage::MessageRelay { payload, .. } => assert_eq!(payload, "second"),
            other => panic!("expected replayed relay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reconnect_ack_fails_when_replay_window_is_truncated() {
        let state = make_state(RemoteConfig {
            replay_buffer_capacity: 1,
            ..RemoteConfig::default()
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("window-agent".into(), vec![], "ws://window".into(), tx)
            .await
            .unwrap();

        for payload in ["one", "two"] {
            assert!(
                state
                    .registry
                    .send_to(
                        "window-agent",
                        ProtocolMessage::MessageRelay {
                            from: "local".into(),
                            to: "window-agent".into(),
                            payload: payload.into(),
                        },
                    )
                    .await
            );
        }

        let _ = rx
            .recv()
            .await
            .expect("initial first delivery should exist");
        let _ = rx
            .recv()
            .await
            .expect("initial second delivery should exist");

        match reconnect_ack(&state.registry, "window-agent", 0).await {
            ProtocolMessage::ReconnectAck {
                success,
                replayed_events,
            } => {
                assert!(!success);
                assert_eq!(replayed_events, 0);
            }
            other => panic!("expected reconnect ack, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reconnect_ack_fails_for_unknown_agent() {
        let state = make_state(RemoteConfig::default());

        match reconnect_ack(&state.registry, "ghost", 0).await {
            ProtocolMessage::ReconnectAck {
                success,
                replayed_events,
            } => {
                assert!(!success);
                assert_eq!(replayed_events, 0);
            }
            other => panic!("expected reconnect ack, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn gateway_health_returns_zero_metrics_when_empty() {
        use super::gateway_health;

        let state = make_state(RemoteConfig::default());
        let axum::Json(metrics) = gateway_health(State(state)).await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connects, 0);
        assert_eq!(metrics.total_disconnects, 0);
        assert_eq!(metrics.avg_uptime_secs, 0);
    }

    #[tokio::test]
    async fn gateway_health_reflects_active_connection() {
        use super::gateway_health;

        let state = make_state(RemoteConfig::default());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("health-agent".into(), vec![], "ws://h".into(), tx)
            .await
            .unwrap();

        let axum::Json(metrics) = gateway_health(State(state)).await;
        assert_eq!(metrics.active_connections, 1);
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 0);
    }

    #[tokio::test]
    async fn gateway_health_reflects_disconnect() {
        use super::gateway_health;

        let state = make_state(RemoteConfig::default());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register("disco-agent".into(), vec![], "ws://d".into(), tx)
            .await
            .unwrap();
        state.registry.unregister("disco-agent").await;

        let axum::Json(metrics) = gateway_health(State(state)).await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 1);
    }
}
