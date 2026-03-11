use anyhow::{Result, bail};
use clap::Subcommand;
use serde::Deserialize;
use std::time::Duration;

/// Default base URL for the OpenGoose web server.
const DEFAULT_BASE: &str = "http://127.0.0.1:8080";
const CLIENT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
const MAX_RECONNECT_BACKOFF_SECS: u64 = 5;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsWrite = futures_util::stream::SplitSink<WsStream, tokio_tungstenite::tungstenite::Message>;
type WsRead = futures_util::stream::SplitStream<WsStream>;

#[derive(Subcommand)]
/// Subcommands for `opengoose remote`.
pub enum RemoteAction {
    /// Connect to an OpenGoose server as a remote agent
    Connect {
        /// WebSocket URL of the OpenGoose server (e.g. ws://localhost:8080)
        url: String,
        /// API key for authentication
        #[arg(long)]
        key: Option<String>,
        /// Agent name to register as
        #[arg(long)]
        name: String,
    },
    /// List connected remote agents
    List {
        /// Base URL of the web server (default: http://127.0.0.1:8080)
        #[arg(long, default_value = DEFAULT_BASE)]
        url: String,
    },
    /// Disconnect a remote agent by name
    Disconnect {
        /// Name of the remote agent to disconnect
        name: String,
        /// Base URL of the web server (default: http://127.0.0.1:8080)
        #[arg(long, default_value = DEFAULT_BASE)]
        url: String,
    },
}

#[derive(Deserialize)]
struct RemoteAgentInfo {
    name: String,
    capabilities: Vec<String>,
    endpoint: String,
    connected_secs: u64,
    last_heartbeat_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectMode {
    Fresh,
    Resume { last_event_id: u64 },
}

struct SessionConnection {
    read: WsRead,
    write: WsWrite,
    replayed_events: u64,
}

#[derive(Debug)]
enum ConnectFailure {
    Retryable(anyhow::Error),
    Terminal(anyhow::Error),
}

enum SessionOutcome {
    Exit,
    Reconnect { reason: String },
}

/// Dispatch and execute the selected remote subcommand.
pub async fn execute(action: RemoteAction) -> Result<()> {
    match action {
        RemoteAction::Connect { url, key, name } => cmd_connect(&url, key.as_deref(), &name).await,
        RemoteAction::List { url } => cmd_list(&url).await,
        RemoteAction::Disconnect { name, url } => cmd_disconnect(&name, &url).await,
    }
}

/// Connect to an OpenGoose server as a remote agent via WebSocket.
async fn cmd_connect(url: &str, api_key: Option<&str>, agent_name: &str) -> Result<()> {
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

fn build_connect_url(url: &str) -> String {
    if url.ends_with("/api/agents/connect") {
        url.to_string()
    } else {
        format!("{}/api/agents/connect", url.trim_end_matches('/'))
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs((1_u64 << attempt.min(3)).min(MAX_RECONNECT_BACKOFF_SECS))
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn heartbeat_message() -> opengoose_teams::remote::ProtocolMessage {
    opengoose_teams::remote::ProtocolMessage::Heartbeat {
        timestamp: now_unix_secs(),
    }
}

fn is_replayable_message(message: &opengoose_teams::remote::ProtocolMessage) -> bool {
    matches!(
        message,
        opengoose_teams::remote::ProtocolMessage::MessageRelay { .. }
            | opengoose_teams::remote::ProtocolMessage::Broadcast { .. }
            | opengoose_teams::remote::ProtocolMessage::Disconnect { .. }
    )
}

fn record_replayable_event(
    last_seen_event_id: &mut u64,
    message: &opengoose_teams::remote::ProtocolMessage,
) -> Option<u64> {
    if is_replayable_message(message) {
        *last_seen_event_id = last_seen_event_id.saturating_add(1);
        Some(*last_seen_event_id)
    } else {
        None
    }
}

fn next_delivery_label(pending_replayed_events: &mut u64) -> &'static str {
    if *pending_replayed_events > 0 {
        *pending_replayed_events -= 1;
        "replay"
    } else {
        "live"
    }
}

async fn send_protocol(
    write: &mut WsWrite,
    message: &opengoose_teams::remote::ProtocolMessage,
) -> Result<()> {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let json = serde_json::to_string(message)?;
    write.send(Message::Text(json.into())).await?;
    Ok(())
}

async fn recv_protocol(
    read: &mut WsRead,
    phase: &str,
) -> std::result::Result<opengoose_teams::remote::ProtocolMessage, ConnectFailure> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let message = match read.next().await {
        Some(Ok(message)) => message,
        Some(Err(err)) => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "websocket error during {phase}: {err}"
            )));
        }
        None => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "connection closed during {phase}"
            )));
        }
    };

    let text = match message {
        Message::Text(text) => text,
        Message::Close(_) => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "connection closed during {phase}"
            )));
        }
        other => {
            return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                "unexpected {phase} response: {other:?}"
            )));
        }
    };

    serde_json::from_str(&text)
        .map_err(|err| ConnectFailure::Terminal(anyhow::anyhow!("invalid {phase} response: {err}")))
}

async fn connect_session(
    ws_url: &str,
    api_key: Option<&str>,
    agent_name: &str,
    connect_mode: ConnectMode,
) -> std::result::Result<SessionConnection, ConnectFailure> {
    use futures_util::StreamExt;

    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|err| {
            ConnectFailure::Retryable(anyhow::anyhow!("failed to connect to {}: {}", ws_url, err))
        })?;

    let (mut write, mut read) = ws_stream.split();
    send_protocol(
        &mut write,
        &opengoose_teams::remote::ProtocolMessage::Handshake {
            agent_name: agent_name.to_string(),
            api_key: api_key.unwrap_or("").to_string(),
            capabilities: vec![],
        },
    )
    .await
    .map_err(|err| ConnectFailure::Retryable(anyhow::anyhow!("failed to send handshake: {err}")))?;

    let ack = recv_protocol(&mut read, "handshake").await?;
    match ack {
        opengoose_teams::remote::ProtocolMessage::HandshakeAck { success: true, .. } => {}
        opengoose_teams::remote::ProtocolMessage::HandshakeAck {
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
        send_protocol(
            &mut write,
            &opengoose_teams::remote::ProtocolMessage::Reconnect { last_event_id },
        )
        .await
        .map_err(|err| {
            ConnectFailure::Retryable(anyhow::anyhow!("failed to send reconnect request: {err}"))
        })?;

        let ack = recv_protocol(&mut read, "reconnect").await?;
        match ack {
            opengoose_teams::remote::ProtocolMessage::ReconnectAck {
                success: true,
                replayed_events: count,
            } => {
                replayed_events = count;
            }
            opengoose_teams::remote::ProtocolMessage::ReconnectAck { success: false, .. } => {
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

async fn cmd_list(base_url: &str) -> Result<()> {
    let url = format!("{}/api/agents/remote", base_url.trim_end_matches('/'));
    let resp = reqwest::get(&url).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to connect to web server at {base_url}: {e}\nIs `opengoose web` running?"
        )
    })?;

    if !resp.status().is_success() {
        bail!(
            "server returned {} when listing remote agents",
            resp.status()
        );
    }

    let agents: Vec<RemoteAgentInfo> = resp.json().await?;

    if agents.is_empty() {
        println!("No remote agents connected.");
        return Ok(());
    }

    println!(
        "{:<20} {:<24} {:<12} {:<12} CAPABILITIES",
        "NAME", "ENDPOINT", "CONNECTED", "HEARTBEAT"
    );
    for agent in &agents {
        println!(
            "{:<20} {:<24} {:<12} {:<12} {}",
            agent.name,
            agent.endpoint,
            format_duration(agent.connected_secs),
            format_duration(agent.last_heartbeat_secs),
            agent.capabilities.join(", "),
        );
    }

    println!("\n{} remote agent(s) connected.", agents.len());
    Ok(())
}

async fn cmd_disconnect(name: &str, base_url: &str) -> Result<()> {
    let url = format!(
        "{}/api/agents/remote/{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(name)
    );
    let client = reqwest::Client::new();
    let resp = client.delete(&url).send().await.map_err(|e| {
        anyhow::anyhow!(
            "failed to connect to web server at {base_url}: {e}\nIs `opengoose web` running?"
        )
    })?;

    if resp.status().is_success() {
        println!("Disconnected remote agent '{name}'.");
    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("remote agent '{name}' is not connected");
    } else {
        bail!("server returned {}", resp.status());
    }

    Ok(())
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use opengoose_teams::remote::ProtocolMessage;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0), "0s ago");
        assert_eq!(format_duration(1), "1s ago");
        assert_eq!(format_duration(59), "59s ago");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60), "1m ago");
        assert_eq!(format_duration(90), "1m ago");
        assert_eq!(format_duration(120), "2m ago");
        assert_eq!(format_duration(3599), "59m ago");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600), "1h ago");
        assert_eq!(format_duration(7200), "2h ago");
        assert_eq!(format_duration(36000), "10h ago");
    }

    #[test]
    fn ws_url_appends_connect_path_when_not_present() {
        assert_eq!(
            build_connect_url("ws://localhost:8080"),
            "ws://localhost:8080/api/agents/connect"
        );
    }

    #[test]
    fn ws_url_preserves_full_connect_path() {
        assert_eq!(
            build_connect_url("ws://localhost:8080/api/agents/connect"),
            "ws://localhost:8080/api/agents/connect"
        );
    }

    #[test]
    fn ws_url_trims_trailing_slash_before_appending() {
        assert_eq!(
            build_connect_url("ws://localhost:8080/"),
            "ws://localhost:8080/api/agents/connect"
        );
    }

    #[test]
    fn list_url_construction() {
        let base_url = "http://127.0.0.1:8080";
        let url = format!("{}/api/agents/remote", base_url.trim_end_matches('/'));
        assert_eq!(url, "http://127.0.0.1:8080/api/agents/remote");
    }

    #[test]
    fn list_url_construction_trims_trailing_slash() {
        let base_url = "http://127.0.0.1:8080/";
        let url = format!("{}/api/agents/remote", base_url.trim_end_matches('/'));
        assert_eq!(url, "http://127.0.0.1:8080/api/agents/remote");
    }

    #[test]
    fn disconnect_url_construction_encodes_name() {
        let base_url = "http://127.0.0.1:8080";
        let name = "my agent";
        let url = format!(
            "{}/api/agents/remote/{}",
            base_url.trim_end_matches('/'),
            urlencoding::encode(name)
        );
        assert_eq!(url, "http://127.0.0.1:8080/api/agents/remote/my%20agent");
    }

    #[test]
    fn remote_agent_info_deserializes_correctly() {
        let json = r#"{
            "name": "test-agent",
            "capabilities": ["chat", "code"],
            "endpoint": "ws://localhost:9000",
            "connected_secs": 120,
            "last_heartbeat_secs": 5
        }"#;
        let info: RemoteAgentInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "test-agent");
        assert_eq!(info.capabilities, vec!["chat", "code"]);
        assert_eq!(info.endpoint, "ws://localhost:9000");
        assert_eq!(info.connected_secs, 120);
        assert_eq!(info.last_heartbeat_secs, 5);
    }

    #[test]
    fn remote_agent_info_formats_duration_for_display() {
        let info = RemoteAgentInfo {
            name: "agent".into(),
            capabilities: vec![],
            endpoint: "ws://localhost:9000".into(),
            connected_secs: 3660,
            last_heartbeat_secs: 30,
        };
        assert_eq!(format_duration(info.connected_secs), "1h ago");
        assert_eq!(format_duration(info.last_heartbeat_secs), "30s ago");
    }

    #[test]
    fn wss_url_appends_connect_path() {
        assert_eq!(
            build_connect_url("wss://example.com:8443"),
            "wss://example.com:8443/api/agents/connect"
        );
    }

    #[test]
    fn wss_url_preserves_full_connect_path() {
        assert_eq!(
            build_connect_url("wss://example.com:8443/api/agents/connect"),
            "wss://example.com:8443/api/agents/connect"
        );
    }

    #[test]
    fn wss_url_trims_trailing_slash_before_appending() {
        assert_eq!(
            build_connect_url("wss://example.com:8443/"),
            "wss://example.com:8443/api/agents/connect"
        );
    }

    #[test]
    fn reconnect_delay_caps_after_backoff_growth() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(2));
        assert_eq!(reconnect_delay(2), Duration::from_secs(4));
        assert_eq!(reconnect_delay(3), Duration::from_secs(5));
        assert_eq!(reconnect_delay(8), Duration::from_secs(5));
    }

    #[test]
    fn record_replayable_event_only_counts_replayable_messages() {
        let mut last_seen_event_id = 0;

        assert_eq!(
            record_replayable_event(
                &mut last_seen_event_id,
                &ProtocolMessage::MessageRelay {
                    from: "a".into(),
                    to: "b".into(),
                    payload: "hello".into(),
                },
            ),
            Some(1)
        );
        assert_eq!(
            record_replayable_event(
                &mut last_seen_event_id,
                &ProtocolMessage::Heartbeat { timestamp: 0 },
            ),
            None
        );
        assert_eq!(
            record_replayable_event(
                &mut last_seen_event_id,
                &ProtocolMessage::Broadcast {
                    from: "ops".into(),
                    channel: "alerts".into(),
                    payload: "hi".into(),
                },
            ),
            Some(2)
        );
    }

    #[test]
    fn next_delivery_label_consumes_pending_replays() {
        let mut pending = 2;
        assert_eq!(next_delivery_label(&mut pending), "replay");
        assert_eq!(next_delivery_label(&mut pending), "replay");
        assert_eq!(next_delivery_label(&mut pending), "live");
        assert_eq!(pending, 0);
    }

    async fn recv_protocol_message(
        socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> ProtocolMessage {
        let message = socket
            .next()
            .await
            .expect("socket should yield a message")
            .expect("socket message should be valid");
        let text = message.into_text().expect("message should be text");
        serde_json::from_str(&text).expect("message should deserialize")
    }

    #[tokio::test]
    async fn connect_session_sends_reconnect_request_with_last_seen_event_id() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();

            match recv_protocol_message(&mut socket).await {
                ProtocolMessage::Handshake {
                    agent_name,
                    api_key,
                    capabilities,
                } => {
                    assert_eq!(agent_name, "resume-agent");
                    assert_eq!(api_key, "secret");
                    assert!(capabilities.is_empty());
                }
                other => panic!("expected handshake, got {other:?}"),
            }

            socket
                .send(Message::Text(
                    serde_json::to_string(&ProtocolMessage::HandshakeAck {
                        success: true,
                        error: None,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();

            match recv_protocol_message(&mut socket).await {
                ProtocolMessage::Reconnect { last_event_id } => {
                    assert_eq!(last_event_id, 7);
                }
                other => panic!("expected reconnect, got {other:?}"),
            }

            socket
                .send(Message::Text(
                    serde_json::to_string(&ProtocolMessage::ReconnectAck {
                        success: true,
                        replayed_events: 2,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
        });

        let session = connect_session(
            &build_connect_url(&format!("ws://{}", addr)),
            Some("secret"),
            "resume-agent",
            ConnectMode::Resume { last_event_id: 7 },
        )
        .await
        .expect("resume handshake should succeed");

        assert_eq!(session.replayed_events, 2);
        drop(session);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_session_returns_terminal_error_when_resume_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();

            let _ = recv_protocol_message(&mut socket).await;
            socket
                .send(Message::Text(
                    serde_json::to_string(&ProtocolMessage::HandshakeAck {
                        success: true,
                        error: None,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();

            let _ = recv_protocol_message(&mut socket).await;
            socket
                .send(Message::Text(
                    serde_json::to_string(&ProtocolMessage::ReconnectAck {
                        success: false,
                        replayed_events: 0,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
        });

        let err = match connect_session(
            &build_connect_url(&format!("ws://{}", addr)),
            None,
            "resume-agent",
            ConnectMode::Resume { last_event_id: 3 },
        )
        .await
        {
            Ok(_) => panic!("resume rejection should be terminal"),
            Err(err) => err,
        };

        match err {
            ConnectFailure::Terminal(err) => {
                assert!(err.to_string().contains("resume rejected"));
            }
            ConnectFailure::Retryable(err) => {
                panic!("expected terminal error, got retryable: {err}");
            }
        }

        server.await.unwrap();
    }
}
