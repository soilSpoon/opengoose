use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::GatewayBridge;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

use crate::types::*;

/// Safety split at 4000 chars for readability (Slack allows ~40k).
const SLACK_MAX_LEN: usize = 4000;

/// Maximum number of consecutive reconnect attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Slack channel gateway using Socket Mode (WebSocket) + Web API.
///
/// Uses `GatewayBridge` for shared orchestration (team intercept, persistence, pairing).
pub struct SlackGateway {
    app_token: String,
    bot_token: String,
    client: reqwest::Client,
    bridge: Arc<GatewayBridge>,
    event_bus: EventBus,
}

impl SlackGateway {
    pub fn new(
        app_token: impl Into<String>,
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            app_token: app_token.into(),
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            bridge,
            event_bus,
        }
    }

    /// Open a Socket Mode WebSocket connection.
    async fn connect_websocket(&self) -> anyhow::Result<String> {
        let resp: ConnectionsOpenResponse = self
            .client
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(&self.app_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "apps.connections.open failed: {}",
                resp.error.unwrap_or_default()
            );
        }

        resp.url.ok_or_else(|| anyhow::anyhow!("no WebSocket URL in response"))
    }

    /// Send a message to a Slack channel via Web API.
    async fn post_message(&self, channel: &str, text: &str) -> anyhow::Result<()> {
        for chunk in split_message(text, SLACK_MAX_LEN) {
            let resp: PostMessageResponse = self
                .client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(&self.bot_token)
                .json(&serde_json::json!({
                    "channel": channel,
                    "text": chunk,
                }))
                .send()
                .await?
                .json()
                .await?;

            if !resp.ok {
                warn!(
                    "chat.postMessage failed: {}",
                    resp.error.unwrap_or_default()
                );
            }
        }
        Ok(())
    }

    /// Respond ephemerally to a slash command via response_url.
    async fn respond_ephemeral(&self, response_url: &str, text: &str) {
        let _ = self
            .client
            .post(response_url)
            .json(&serde_json::json!({
                "response_type": "ephemeral",
                "text": text,
            }))
            .send()
            .await;
    }

    /// Handle the /team slash command.
    async fn handle_team_command(&self, cmd: &SlashCommand) {
        let Some(channel_id) = cmd.channel_id.as_deref() else {
            return;
        };
        let team_id = cmd.team_id.as_deref().unwrap_or("unknown");
        let session_key = SessionKey::new(Platform::Slack, team_id, channel_id);
        let engine = self.bridge.engine();

        let args = cmd.text.as_deref().unwrap_or("").trim();

        let response = match args {
            "" => match engine.active_team_for(&session_key) {
                Some(team) => format!("Active team for this channel: *{team}*"),
                None => "No team active for this channel.".to_string(),
            },
            "off" => {
                engine.clear_active_team(&session_key);
                "Team deactivated. Reverting to single-agent mode.".to_string()
            }
            "list" => {
                let teams = engine.list_teams();
                if teams.is_empty() {
                    "No teams available.".to_string()
                } else {
                    format!(
                        "Available teams:\n{}",
                        teams
                            .iter()
                            .map(|t| format!("• {t}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            team_name => {
                if engine.team_exists(team_name) {
                    engine.set_active_team(&session_key, team_name.to_string());
                    format!("Team *{team_name}* activated for this channel.")
                } else {
                    let available = engine.list_teams();
                    format!(
                        "Team `{team_name}` not found. Available teams: {}",
                        if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        }
                    )
                }
            }
        };

        if let Some(ref url) = cmd.response_url {
            self.respond_ephemeral(url, &response).await;
        }
    }

    /// Process a single Socket Mode envelope.
    async fn handle_envelope(
        &self,
        envelope: &SocketEnvelope,
        bot_user_id: &str,
    ) {
        let Some(ref payload) = envelope.payload else {
            return;
        };

        match envelope.envelope_type.as_str() {
            "events_api" => {
                let Ok(callback) = serde_json::from_value::<EventCallback>(payload.clone())
                else {
                    return;
                };

                let Some(event) = callback.event else {
                    return;
                };

                // Only handle regular messages (no bot messages, no subtypes like joins)
                if event.event_type != "message" || event.subtype.is_some() {
                    return;
                }

                // Ignore bot messages
                if event.bot_id.is_some() {
                    return;
                }
                if event.user.as_deref() == Some(bot_user_id) {
                    return;
                }

                let Some(channel) = event.channel.as_deref() else {
                    return;
                };
                let Some(text) = event.text.as_deref() else {
                    return;
                };

                let text = text.trim();
                if text.is_empty() {
                    return;
                }

                let team_id = callback.team_id.as_deref().unwrap_or("unknown");
                let session_key = SessionKey::new(Platform::Slack, team_id, channel);
                let display_name = event.user.clone();

                match self
                    .bridge
                    .relay_message(&session_key, display_name, text)
                    .await
                {
                    Ok(Some(response)) => {
                        if let Err(e) = self.post_message(channel, &response).await {
                            error!(%e, "failed to send team response to slack");
                        }
                    }
                    Ok(None) => {
                        // Goose single-agent — response comes via send_message callback
                    }
                    Err(e) => {
                        self.event_bus.emit(AppEventKind::Error {
                            context: "relay".into(),
                            message: e.to_string(),
                        });
                        error!(%e, "failed to relay slack message");
                    }
                }
            }
            "slash_commands" => {
                let Ok(cmd) = serde_json::from_value::<SlashCommand>(payload.clone()) else {
                    return;
                };

                if cmd.command.as_deref() == Some("/team") {
                    self.handle_team_command(&cmd).await;
                }
            }
            _ => {}
        }
    }

    /// Get the bot's user ID for filtering self-messages.
    async fn get_bot_user_id(&self) -> anyhow::Result<String> {
        let resp: AuthTestResponse = self
            .client
            .post("https://slack.com/api/auth.test")
            .bearer_auth(&self.bot_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("auth.test failed: {}", resp.error.unwrap_or_default());
        }

        resp.user_id
            .ok_or_else(|| anyhow::anyhow!("auth.test returned no user_id"))
    }

    /// Run the WebSocket event loop with reconnection support.
    async fn run_socket_mode(
        &self,
        cancel: &CancellationToken,
        bot_user_id: &str,
    ) -> anyhow::Result<()> {
        let mut reconnect_attempts: u32 = 0;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            // Get a new WebSocket URL
            let ws_url = match self.connect_websocket().await {
                Ok(url) => {
                    reconnect_attempts = 0;
                    url
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                        return Err(e);
                    }
                    let delay = std::time::Duration::from_secs(2u64.pow(reconnect_attempts.min(5)));
                    warn!(%e, ?delay, "failed to get WebSocket URL, retrying...");
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            info!("connecting to slack socket mode");

            let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                Ok(conn) => conn,
                Err(e) => {
                    reconnect_attempts += 1;
                    if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                        return Err(e.into());
                    }
                    let delay = std::time::Duration::from_secs(2u64.pow(reconnect_attempts.min(5)));
                    warn!(%e, ?delay, "WebSocket connect failed, retrying...");
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            let (mut ws_write, mut ws_read) = ws_stream.split();

            info!("slack socket mode connected");

            // Process messages until disconnect
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = ws_write.close().await;
                        return Ok(());
                    }
                    msg = ws_read.next() => {
                        match msg {
                            Some(Ok(WsMessage::Text(text))) => {
                                let Ok(envelope) = serde_json::from_str::<SocketEnvelope>(&text) else {
                                    continue;
                                };

                                // ACK immediately (must be within 5 seconds)
                                let ack = EnvelopeAck {
                                    envelope_id: envelope.envelope_id.clone(),
                                    payload: None,
                                };
                                if let Ok(ack_json) = serde_json::to_string(&ack) {
                                    if ws_write.send(WsMessage::Text(ack_json.into())).await.is_err() {
                                        warn!("failed to send ACK, reconnecting...");
                                        break;
                                    }
                                }

                                // Handle the envelope
                                self.handle_envelope(&envelope, bot_user_id).await;
                            }
                            Some(Ok(WsMessage::Ping(data))) => {
                                let _ = ws_write.send(WsMessage::Pong(data)).await;
                            }
                            Some(Ok(WsMessage::Close(_))) | None => {
                                info!("slack WebSocket closed, reconnecting...");
                                break;
                            }
                            Some(Err(e)) => {
                                warn!(%e, "slack WebSocket error, reconnecting...");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Small delay before reconnect
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        Ok(())
    }
}

#[async_trait]
impl Gateway for SlackGateway {
    fn gateway_type(&self) -> &str {
        "slack"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        self.bridge.on_start(handler).await;

        // Verify bot token and get bot user ID
        let bot_user_id = self.get_bot_user_id().await?;
        info!(bot_user_id = %bot_user_id, "slack gateway starting");

        self.event_bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Slack,
        });

        match self.run_socket_mode(&cancel, &bot_user_id).await {
            Ok(()) => {}
            Err(e) => {
                error!(%e, "slack socket mode failed");
                self.event_bus.emit(AppEventKind::ChannelDisconnected {
                    platform: Platform::Slack,
                    reason: e.to_string(),
                });
            }
        }

        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Slack,
            reason: "shutdown".into(),
        });

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            self.bridge
                .on_outgoing_message(&user.user_id, &body, "slack")
                .await;

            let session_key = SessionKey::from_stable_id(&user.user_id);
            if let Err(e) = self.post_message(&session_key.channel_id, &body).await {
                error!(%e, "failed to send slack message");
            }
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        // Verify bot token
        let resp: AuthTestResponse = self
            .client
            .post("https://slack.com/api/auth.test")
            .bearer_auth(&self.bot_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "Slack bot token validation failed: {}",
                resp.error.unwrap_or_default()
            );
        }

        Ok(())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "slack".into())])
    }
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }
        let mut boundary = max_len;
        while !remaining.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let split_at = remaining[..boundary].rfind('\n').unwrap_or(boundary);
        chunks.push(&remaining[..split_at]);
        remaining = remaining[split_at..].trim_start_matches('\n');
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short() {
        assert_eq!(split_message("hi", SLACK_MAX_LEN), vec!["hi"]);
    }

    #[test]
    fn test_split_long() {
        let msg = "a".repeat(5000);
        let chunks = split_message(&msg, SLACK_MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), SLACK_MAX_LEN);
        assert_eq!(chunks[1].len(), 1000);
    }
}
