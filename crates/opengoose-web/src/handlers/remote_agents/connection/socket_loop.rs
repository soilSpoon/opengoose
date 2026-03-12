use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use opengoose_teams::remote::ProtocolMessage;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::super::RemoteGatewayState;
use super::lifecycle::reconnect_ack;
use super::socket_io::{log_socket_error, send_protocol};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionDirective {
    Continue,
    GracefulDisconnect,
}

pub(super) async fn run_connection_loop(
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
