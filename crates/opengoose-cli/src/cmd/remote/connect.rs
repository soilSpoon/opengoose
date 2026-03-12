use anyhow::Result;
use std::time::Duration;

use super::protocol::{
    WsRead, WsWrite, heartbeat_message, next_delivery_label, record_replayable_event,
    recv_protocol, send_protocol,
};

const CLIENT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
const MAX_RECONNECT_BACKOFF_SECS: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConnectMode {
    Fresh,
    Resume { last_event_id: u64 },
}

pub(super) struct SessionConnection {
    pub(super) read: WsRead,
    pub(super) write: WsWrite,
    pub(super) replayed_events: u64,
}

#[derive(Debug)]
pub(super) enum ConnectFailure {
    Retryable(anyhow::Error),
    Terminal(anyhow::Error),
}

enum SessionOutcome {
    Exit,
    Reconnect { reason: String },
}

/// Connect to an OpenGoose server as a remote agent via WebSocket.
pub(super) async fn cmd_connect(url: &str, api_key: Option<&str>, agent_name: &str) -> Result<()> {
    let ws_url = build_connect_url(url);
    let mut connect_mode = ConnectMode::Fresh;
    let mut last_seen_event_id = 0_u64;
    let mut reconnect_attempt = 0_u32;

    loop {
        match connect_mode {
            ConnectMode::Fresh => {
                println!("Connecting to {} as '{}'...", ws_url, agent_name);
            }
            ConnectMode::Resume { last_event_id } => {
                println!(
                    "Reconnecting to {} as '{}' from event #{}...",
                    ws_url, agent_name, last_event_id
                );
            }
        }

        let session = match connect_session(&ws_url, api_key, agent_name, connect_mode).await {
            Ok(session) => session,
            Err(ConnectFailure::Retryable(err))
                if matches!(connect_mode, ConnectMode::Resume { .. }) =>
            {
                eprintln!(
                    "Reconnect attempt {} failed: {}",
                    reconnect_attempt.saturating_add(1),
                    err
                );
                let delay = reconnect_delay(reconnect_attempt);
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                if !wait_before_reconnect(delay, last_seen_event_id).await {
                    break;
                }
                continue;
            }
            Err(ConnectFailure::Retryable(err)) | Err(ConnectFailure::Terminal(err)) => {
                return Err(err);
            }
        };

        reconnect_attempt = 0;
        match connect_mode {
            ConnectMode::Fresh => println!("Connected successfully as '{}'.", agent_name),
            ConnectMode::Resume { .. } => println!(
                "Resumed as '{}' with {} replayed event(s).",
                agent_name, session.replayed_events
            ),
        }
        println!("Listening for messages (press Ctrl+C to disconnect)...");

        let mut pending_replayed_events = session.replayed_events;
        match run_connected_session(
            session.read,
            session.write,
            &mut last_seen_event_id,
            &mut pending_replayed_events,
        )
        .await?
        {
            SessionOutcome::Exit => break,
            SessionOutcome::Reconnect { reason } => {
                println!("Connection lost: {}", reason);
                connect_mode = ConnectMode::Resume {
                    last_event_id: last_seen_event_id,
                };
                let delay = reconnect_delay(reconnect_attempt);
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                if !wait_before_reconnect(delay, last_seen_event_id).await {
                    break;
                }
            }
        }
    }

    Ok(())
}

pub(super) fn build_connect_url(url: &str) -> String {
    if url.ends_with("/api/agents/connect") {
        url.to_string()
    } else {
        format!("{}/api/agents/connect", url.trim_end_matches('/'))
    }
}

pub(super) fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs((1_u64 << attempt.min(3)).min(MAX_RECONNECT_BACKOFF_SECS))
}

pub(super) async fn connect_session(
    ws_url: &str,
    api_key: Option<&str>,
    agent_name: &str,
    connect_mode: ConnectMode,
) -> std::result::Result<SessionConnection, ConnectFailure> {
    use futures_util::StreamExt;
    use opengoose_teams::remote::ProtocolMessage;

    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|err| {
            ConnectFailure::Retryable(anyhow::anyhow!("failed to connect to {}: {}", ws_url, err))
        })?;

    let (mut write, mut read) = ws_stream.split();
    send_protocol(
        &mut write,
        &ProtocolMessage::Handshake {
            agent_name: agent_name.to_string(),
            api_key: api_key.unwrap_or("").to_string(),
            capabilities: vec![],
        },
    )
    .await
    .map_err(|err| ConnectFailure::Retryable(anyhow::anyhow!("failed to send handshake: {err}")))?;

    let ack = recv_protocol(&mut read, "handshake").await?;
    match ack {
        ProtocolMessage::HandshakeAck { success: true, .. } => {}
        ProtocolMessage::HandshakeAck {
            success: false,
            error,
            ..
        } => {
            return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                "handshake rejected: {}",
                error.unwrap_or_else(|| "unknown error".into())
            )));
        }
        _ => {
            return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                "unexpected handshake response"
            )));
        }
    }

    let mut replayed_events = 0;
    if let ConnectMode::Resume { last_event_id } = connect_mode {
        send_protocol(&mut write, &ProtocolMessage::Reconnect { last_event_id })
            .await
            .map_err(|err| {
                ConnectFailure::Retryable(anyhow::anyhow!(
                    "failed to send reconnect request: {err}"
                ))
            })?;

        let ack = recv_protocol(&mut read, "reconnect").await?;
        match ack {
            ProtocolMessage::ReconnectAck {
                success: true,
                replayed_events: count,
            } => {
                replayed_events = count;
            }
            ProtocolMessage::ReconnectAck { success: false, .. } => {
                return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                    "resume rejected: replay window unavailable after event #{}",
                    last_event_id
                )));
            }
            _ => {
                return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                    "unexpected reconnect response"
                )));
            }
        }
    }

    Ok(SessionConnection {
        read,
        write,
        replayed_events,
    })
}

async fn run_connected_session(
    mut read: WsRead,
    mut write: WsWrite,
    last_seen_event_id: &mut u64,
    pending_replayed_events: &mut u64,
) -> Result<SessionOutcome> {
    use futures_util::StreamExt;
    use opengoose_teams::remote::ProtocolMessage;
    use tokio_tungstenite::tungstenite::Message;

    let mut heartbeat_timer = tokio::time::interval(CLIENT_HEARTBEAT_INTERVAL);
    heartbeat_timer.tick().await;

    loop {
        tokio::select! {
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ProtocolMessage>(&text) {
                            Ok(message) => {
                                let event_id = record_replayable_event(last_seen_event_id, &message);
                                let delivery_label = event_id.map(|_| next_delivery_label(pending_replayed_events));
                                match message {
                                    ProtocolMessage::Heartbeat { .. } => {
                                        send_protocol(&mut write, &heartbeat_message()).await?;
                                    }
                                    ProtocolMessage::MessageRelay { from, payload, .. } => {
                                        let event_id = event_id.expect("relay events should have an id");
                                        println!(
                                            "[{} relay #{} from {}] {}",
                                            delivery_label.unwrap_or("live"),
                                            event_id,
                                            from,
                                            payload
                                        );
                                    }
                                    ProtocolMessage::Broadcast { from, channel, payload } => {
                                        let event_id = event_id.expect("broadcast events should have an id");
                                        println!(
                                            "[{} broadcast #{} {}@{}] {}",
                                            delivery_label.unwrap_or("live"),
                                            event_id,
                                            from,
                                            channel,
                                            payload
                                        );
                                    }
                                    ProtocolMessage::Disconnect { reason } => {
                                        let event_id = event_id.expect("disconnect events should have an id");
                                        println!(
                                            "[{} disconnect #{}] Server disconnected: {}",
                                            delivery_label.unwrap_or("live"),
                                            event_id,
                                            reason
                                        );
                                        return Ok(SessionOutcome::Exit);
                                    }
                                    ProtocolMessage::Error { message } => {
                                        eprintln!("Server error: {}", message);
                                    }
                                    ProtocolMessage::Handshake { .. }
                                    | ProtocolMessage::HandshakeAck { .. }
                                    | ProtocolMessage::Reconnect { .. }
                                    | ProtocolMessage::ReconnectAck { .. } => {}
                                }
                            }
                            Err(err) => eprintln!("Invalid message: {}", err),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Ok(SessionOutcome::Reconnect {
                            reason: "connection closed".into(),
                        });
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        return Ok(SessionOutcome::Reconnect {
                            reason: format!("websocket error: {}", err),
                        });
                    }
                }
            }
            _ = heartbeat_timer.tick() => {
                if send_protocol(&mut write, &heartbeat_message()).await.is_err() {
                    return Ok(SessionOutcome::Reconnect {
                        reason: "heartbeat send failed".into(),
                    });
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nDisconnecting...");
                let _ = send_protocol(
                    &mut write,
                    &ProtocolMessage::Disconnect {
                        reason: "user interrupt".into(),
                    },
                )
                .await;
                return Ok(SessionOutcome::Exit);
            }
        }
    }
}

async fn wait_before_reconnect(delay: Duration, last_seen_event_id: u64) -> bool {
    println!(
        "Retrying reconnect in {}s from event #{} (press Ctrl+C to stop)...",
        delay.as_secs(),
        last_seen_event_id
    );
    tokio::select! {
        _ = tokio::time::sleep(delay) => true,
        _ = tokio::signal::ctrl_c() => {
            println!("\nStopping reconnect attempts.");
            false
        }
    }
}
