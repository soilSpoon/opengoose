use anyhow::{Result, bail};
use clap::Subcommand;
use serde::Deserialize;

/// Default base URL for the OpenGoose web server.
const DEFAULT_BASE: &str = "http://127.0.0.1:8080";

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
    use futures_util::{SinkExt, StreamExt};
    use opengoose_teams::remote::ProtocolMessage;
    use tokio_tungstenite::tungstenite::Message;

    // Build the WebSocket URL for the connect endpoint.
    let ws_url = if url.ends_with("/api/agents/connect") {
        url.to_string()
    } else {
        format!("{}/api/agents/connect", url.trim_end_matches('/'))
    };

    println!("Connecting to {} as '{}'...", ws_url, agent_name);

    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to {}: {}", ws_url, e))?;

    let (mut write, mut read) = ws_stream.split();

    // Send handshake.
    let handshake = ProtocolMessage::Handshake {
        agent_name: agent_name.to_string(),
        api_key: api_key.unwrap_or("").to_string(),
        capabilities: vec![],
    };
    let json = serde_json::to_string(&handshake)?;
    write.send(Message::Text(json.into())).await?;

    // Wait for handshake acknowledgement.
    let ack_msg = read
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("connection closed during handshake"))??;

    let ack_text = ack_msg
        .to_text()
        .map_err(|e| anyhow::anyhow!("non-text handshake response: {}", e))?;

    let ack: ProtocolMessage = serde_json::from_str(ack_text)?;
    match ack {
        ProtocolMessage::HandshakeAck { success: true, .. } => {
            println!("Connected successfully as '{}'.", agent_name);
        }
        ProtocolMessage::HandshakeAck {
            success: false,
            error,
            ..
        } => {
            bail!(
                "handshake rejected: {}",
                error.unwrap_or_else(|| "unknown error".into())
            );
        }
        _ => bail!("unexpected handshake response"),
    }

    println!("Listening for messages (press Ctrl+C to disconnect)...");

    // Main loop: receive messages and respond to heartbeats.
    let mut heartbeat_timer = tokio::time::interval(std::time::Duration::from_secs(25));
    heartbeat_timer.tick().await;

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ProtocolMessage>(&text) {
                            Ok(ProtocolMessage::Heartbeat { .. }) => {
                                let hb = ProtocolMessage::Heartbeat {
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0),
                                };
                                let json = serde_json::to_string(&hb)?;
                                write.send(Message::Text(json.into())).await?;
                            }
                            Ok(ProtocolMessage::MessageRelay { from, payload, .. }) => {
                                println!("[message from {}] {}", from, payload);
                            }
                            Ok(ProtocolMessage::Broadcast { from, channel, payload }) => {
                                println!("[broadcast {}@{}] {}", from, channel, payload);
                            }
                            Ok(ProtocolMessage::Disconnect { reason }) => {
                                println!("Server disconnected: {}", reason);
                                break;
                            }
                            Ok(ProtocolMessage::Error { message }) => {
                                eprintln!("Server error: {}", message);
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("Invalid message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        println!("Connection closed.");
                        break;
                    }
                    Some(Ok(_)) => {} // skip binary/ping/pong
                    Some(Err(e)) => {
                        eprintln!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            _ = heartbeat_timer.tick() => {
                let hb = ProtocolMessage::Heartbeat {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                };
                let json = serde_json::to_string(&hb)?;
                if write.send(Message::Text(json.into())).await.is_err() {
                    println!("Connection lost.");
                    break;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nDisconnecting...");
                let disc = ProtocolMessage::Disconnect {
                    reason: "user interrupt".into(),
                };
                let json = serde_json::to_string(&disc)?;
                let _ = write.send(Message::Text(json.into())).await;
                break;
            }
        }
    }

    Ok(())
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
