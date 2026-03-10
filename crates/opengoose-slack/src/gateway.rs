use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::message_utils::{split_message, truncate_for_display};
use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform, SessionKey};

use crate::types::*;

/// Safety split at 4000 chars for readability (Slack allows ~40k).
const SLACK_MAX_LEN: usize = 4000;

/// Maximum number of consecutive reconnect attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

#[derive(Debug, Clone, PartialEq)]
enum SlackEnvelopeAction {
    Ignore,
    Relay {
        session_key: SessionKey,
        channel: String,
        text: String,
        display_name: String,
    },
    TeamCommand(SlashCommand),
}

/// Slack channel gateway using Socket Mode (WebSocket) + Web API.
///
/// Uses `GatewayBridge` for shared orchestration (team intercept, persistence, pairing).
pub struct SlackGateway {
    app_token: String,
    bot_token: String,
    client: reqwest::Client,
    bridge: Arc<GatewayBridge>,
    event_bus: EventBus,
    metrics: ChannelMetricsStore,
}

impl SlackGateway {
    pub fn new(
        app_token: impl Into<String>,
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> Self {
        Self::with_metrics(
            app_token,
            bot_token,
            bridge,
            event_bus,
            ChannelMetricsStore::new(),
        )
    }

    pub fn with_metrics(
        app_token: impl Into<String>,
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
        metrics: ChannelMetricsStore,
    ) -> Self {
        Self {
            app_token: app_token.into(),
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            bridge,
            event_bus,
            metrics,
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

        resp.url
            .ok_or_else(|| anyhow::anyhow!("no WebSocket URL in response"))
    }

    /// Send a message to a Slack channel via Web API.
    async fn post_message(&self, channel: &str, text: &str) -> anyhow::Result<()> {
        debug!(channel = %channel, text_len = text.len(), "posting slack message");
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

        let args = cmd.text.as_deref().unwrap_or("").trim();
        let response = self.bridge.handle_team_command(&session_key, args);

        if let Some(ref url) = cmd.response_url {
            self.respond_ephemeral(url, &response).await;
        }
    }

    /// Process a single Socket Mode envelope.
    async fn handle_envelope(&self, envelope: &SocketEnvelope, bot_user_id: &str) {
        match classify_slack_envelope(envelope, bot_user_id) {
            SlackEnvelopeAction::Ignore => {
                debug!(envelope_type = %envelope.envelope_type, "ignoring slack envelope");
            }
            SlackEnvelopeAction::Relay {
                session_key,
                channel,
                text,
                display_name,
            } => {
                debug!(
                    channel = %channel,
                    user = %display_name,
                    text_len = text.len(),
                    "relaying slack message to engine"
                );
                if let Err(e) = self
                    .bridge
                    .relay_and_drive_stream(
                        &session_key,
                        Some(display_name),
                        &text,
                        self as &dyn StreamResponder,
                        &channel,
                        opengoose_core::ThrottlePolicy::slack(),
                        SLACK_MAX_LEN,
                    )
                    .await
                {
                    // Error event is emitted by bridge; just log here
                    error!(%e, "failed to relay slack message");
                }
            }
            SlackEnvelopeAction::TeamCommand(ref cmd) => {
                debug!(command = ?cmd.command, "handling slack team command");
                self.handle_team_command(cmd).await;
            }
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
                    let Some(delay) = websocket_reconnect_delay(reconnect_attempts) else {
                        return Err(e);
                    };
                    let delay_secs = delay.as_secs();
                    warn!(%e, ?delay, "failed to get WebSocket URL, retrying...");
                    self.metrics.record_reconnect("slack", Some(e.to_string()));
                    self.event_bus.emit(AppEventKind::ChannelReconnecting {
                        platform: Platform::Slack,
                        attempt: reconnect_attempts,
                        delay_secs,
                    });
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            info!("connecting to slack socket mode");

            let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                Ok(conn) => conn,
                Err(e) => {
                    reconnect_attempts += 1;
                    let Some(delay) = websocket_reconnect_delay(reconnect_attempts) else {
                        return Err(e.into());
                    };
                    let delay_secs = delay.as_secs();
                    warn!(%e, ?delay, "WebSocket connect failed, retrying...");
                    self.metrics.record_reconnect("slack", Some(e.to_string()));
                    self.event_bus.emit(AppEventKind::ChannelReconnecting {
                        platform: Platform::Slack,
                        attempt: reconnect_attempts,
                        delay_secs,
                    });
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            let (mut ws_write, mut ws_read) = ws_stream.split();

            info!("slack socket mode connected");
            self.metrics.set_connected("slack");

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
                                if let Ok(ack_json) = serde_json::to_string(&ack)
                                    && ws_write
                                        .send(WsMessage::Text(ack_json.into()))
                                        .await
                                        .is_err()
                                {
                                    warn!("failed to send ACK, reconnecting...");
                                    break;
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
        self.metrics.set_connected("slack");

        let reason = match self.run_socket_mode(&cancel, &bot_user_id).await {
            Ok(()) => "shutdown".to_string(),
            Err(e) => {
                error!(%e, "slack socket mode failed");
                e.to_string()
            }
        };

        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Slack,
            reason,
        });

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            let session_key = self
                .bridge
                .on_outgoing_message(&user.user_id, &body, "slack")
                .await;

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

#[async_trait]
impl StreamResponder for SlackGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        SLACK_MAX_LEN
    }

    async fn create_draft(&self, channel: &str) -> anyhow::Result<DraftHandle> {
        debug!(channel = %channel, "creating slack draft");
        let resp: PostMessageResponse = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({
                "channel": channel,
                "text": "Thinking...",
            }))
            .send()
            .await?
            .json()
            .await?;

        let ts = resp
            .ts
            .ok_or_else(|| anyhow::anyhow!("chat.postMessage returned no ts"))?;
        debug!(channel = %channel, ts = %ts, "slack draft created");
        Ok(DraftHandle {
            message_id: ts,
            channel_id: channel.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(channel = %handle.channel_id, ts = %handle.message_id, content_len = content.len(), "updating slack draft");
        let display = truncate_for_display(content, SLACK_MAX_LEN);
        let resp: ChatUpdateResponse = self
            .client
            .post("https://slack.com/api/chat.update")
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({
                "channel": handle.channel_id,
                "ts": handle.message_id,
                "text": display,
            }))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("chat.update failed");
        }
        Ok(())
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.post_message(channel_id, content).await
    }

    // finalize_draft uses the default implementation from StreamResponder
}

fn classify_slack_envelope(envelope: &SocketEnvelope, bot_user_id: &str) -> SlackEnvelopeAction {
    let Some(payload) = envelope.payload.as_ref() else {
        return SlackEnvelopeAction::Ignore;
    };

    match envelope.envelope_type.as_str() {
        "events_api" => match serde_json::from_value::<EventCallback>(payload.clone()) {
            Ok(callback) => {
                let Some(event) = callback.event else {
                    return SlackEnvelopeAction::Ignore;
                };

                if event.event_type != "message" || event.subtype.is_some() {
                    return SlackEnvelopeAction::Ignore;
                }

                if event.bot_id.is_some() {
                    return SlackEnvelopeAction::Ignore;
                }

                if event.user.as_deref() == Some(bot_user_id) {
                    return SlackEnvelopeAction::Ignore;
                }

                let Some(channel) = event.channel.as_deref() else {
                    return SlackEnvelopeAction::Ignore;
                };
                let Some(text) = event.text.as_deref().map(str::trim) else {
                    return SlackEnvelopeAction::Ignore;
                };

                if text.is_empty() {
                    return SlackEnvelopeAction::Ignore;
                }

                let Some(display_name) = event.user else {
                    return SlackEnvelopeAction::Ignore;
                };
                let team_id = callback.team_id.as_deref().unwrap_or("unknown");
                SlackEnvelopeAction::Relay {
                    session_key: SessionKey::new(Platform::Slack, team_id, channel),
                    channel: channel.to_string(),
                    text: text.to_string(),
                    display_name,
                }
            }
            Err(_) => SlackEnvelopeAction::Ignore,
        },
        "slash_commands" => match serde_json::from_value::<SlashCommand>(payload.clone()) {
            Ok(cmd) if cmd.command.as_deref() == Some("/team") => {
                SlackEnvelopeAction::TeamCommand(cmd)
            }
            _ => SlackEnvelopeAction::Ignore,
        },
        _ => SlackEnvelopeAction::Ignore,
    }
}

fn websocket_reconnect_delay(attempts: u32) -> Option<std::time::Duration> {
    if attempts >= MAX_RECONNECT_ATTEMPTS {
        None
    } else {
        Some(std::time::Duration::from_secs(2u64.pow(attempts.min(5))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_max_len_constant() {
        assert_eq!(SLACK_MAX_LEN, 4000);
    }

    #[test]
    fn test_max_reconnect_attempts_constant() {
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 10);
    }

    #[test]
    fn test_websocket_reconnect_delay_exhausted() {
        assert!(websocket_reconnect_delay(MAX_RECONNECT_ATTEMPTS).is_none());
        assert!(websocket_reconnect_delay(MAX_RECONNECT_ATTEMPTS - 1).is_some());
    }

    #[test]
    fn test_websocket_reconnect_delay_growth() {
        assert_eq!(
            websocket_reconnect_delay(1).unwrap(),
            std::time::Duration::from_secs(2)
        );
        assert_eq!(
            websocket_reconnect_delay(5).unwrap(),
            std::time::Duration::from_secs(32)
        );
    }

    #[test]
    fn test_slack_envelope_relay_filter_ignores_bot_message() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-1".to_string(),
            envelope_type: "events_api".to_string(),
            payload: Some(serde_json::json!({
                "team_id": "T123",
                "event": {
                    "type": "message",
                    "channel": "C1",
                    "user": "U1",
                    "text": "hello",
                    "bot_id": "B1",
                }
            })),
        };
        assert!(matches!(
            classify_slack_envelope(&envelope, "U1"),
            SlackEnvelopeAction::Ignore
        ));
    }

    #[test]
    fn test_slack_envelope_relay_filter_ignores_subtype() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-2".to_string(),
            envelope_type: "events_api".to_string(),
            payload: Some(serde_json::json!({
                "team_id": "T123",
                "event": {
                    "type": "message",
                    "channel": "C1",
                    "user": "U1",
                    "text": "hello",
                    "subtype": "channel_join"
                }
            })),
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_slack_envelope_relay_message() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-3".to_string(),
            envelope_type: "events_api".to_string(),
            payload: Some(serde_json::json!({
                "team_id": "T123",
                "event": {
                    "type": "message",
                    "channel": "C1",
                    "user": "U2",
                    "text": "   hello   ",
                }
            })),
        };
        let action = classify_slack_envelope(&envelope, "BOT");
        assert!(matches!(
            action,
            SlackEnvelopeAction::Relay {
                session_key,
                channel,
                ref text,
                display_name
            } if session_key.platform == Platform::Slack
                && session_key.namespace == Some("T123".to_string())
                && session_key.channel_id == "C1"
                && channel == "C1"
                && text == "hello"
                && display_name == "U2"
        ));
    }

    #[test]
    fn test_slack_envelope_team_command() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-4".to_string(),
            envelope_type: "slash_commands".to_string(),
            payload: Some(serde_json::json!({
                "command": "/team",
                "text": "ops",
                "channel_id": "C1",
                "team_id": "T123",
            })),
        };

        let action = classify_slack_envelope(&envelope, "BOT");
        let SlackEnvelopeAction::TeamCommand(cmd) = action else {
            panic!("expected team command");
        };
        assert_eq!(cmd.command.as_deref(), Some("/team"));
        assert_eq!(cmd.text.as_deref(), Some("ops"));
    }

    #[test]
    fn test_slack_envelope_ignores_self_message_by_user_id() {
        // Bot's own user_id (not bot_id field) should be filtered out
        let envelope = SocketEnvelope {
            envelope_id: "envelope-5".to_string(),
            envelope_type: "events_api".to_string(),
            payload: Some(serde_json::json!({
                "team_id": "T123",
                "event": {
                    "type": "message",
                    "channel": "C1",
                    "user": "BOT_USER_ID",
                    "text": "I said this myself",
                }
            })),
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT_USER_ID"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_slack_envelope_ignores_empty_text() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-6".to_string(),
            envelope_type: "events_api".to_string(),
            payload: Some(serde_json::json!({
                "team_id": "T123",
                "event": {
                    "type": "message",
                    "channel": "C1",
                    "user": "U1",
                    "text": "   ",
                }
            })),
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_slack_envelope_ignores_unknown_type() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-7".to_string(),
            envelope_type: "hello".to_string(),
            payload: Some(serde_json::json!({"type": "hello"})),
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_slack_envelope_ignores_non_team_slash_command() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-8".to_string(),
            envelope_type: "slash_commands".to_string(),
            payload: Some(serde_json::json!({
                "command": "/other",
                "text": "something",
                "channel_id": "C1",
                "team_id": "T123",
            })),
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_slack_envelope_ignores_no_payload() {
        let envelope = SocketEnvelope {
            envelope_id: "envelope-9".to_string(),
            envelope_type: "events_api".to_string(),
            payload: None,
        };
        assert_eq!(
            classify_slack_envelope(&envelope, "BOT"),
            SlackEnvelopeAction::Ignore
        );
    }

    #[test]
    fn test_websocket_reconnect_delay_full_sequence() {
        // Verify the exponential capped sequence: 2, 4, 8, 16, 32, 32, 32, 32, 32
        let delays: Vec<u64> = (1..MAX_RECONNECT_ATTEMPTS)
            .map(|attempt| websocket_reconnect_delay(attempt).unwrap().as_secs())
            .collect();
        assert_eq!(delays, vec![2, 4, 8, 16, 32, 32, 32, 32, 32]);
    }

    #[test]
    fn test_websocket_reconnect_delay_attempt_zero_is_one_second() {
        assert_eq!(
            websocket_reconnect_delay(0).unwrap(),
            std::time::Duration::from_secs(1)
        );
    }

    #[test]
    fn test_metrics_store_records_reconnect_and_connect() {
        use opengoose_types::ChannelMetricsStore;

        let store = ChannelMetricsStore::new();

        store.record_reconnect("slack", Some("connection refused".into()));
        store.record_reconnect("slack", Some("timeout".into()));

        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 2);
        assert_eq!(snap["slack"].last_error.as_deref(), Some("timeout"));
        assert!(snap["slack"].uptime_secs.is_none());

        // Successful connect clears error and sets uptime
        store.set_connected("slack");
        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 2); // count preserved
        assert!(snap["slack"].last_error.is_none()); // error cleared
        assert!(snap["slack"].uptime_secs.is_some()); // uptime set
    }

    #[test]
    fn test_event_bus_emits_channel_reconnecting() {
        use opengoose_types::{AppEventKind, EventBus, Platform};

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(AppEventKind::ChannelReconnecting {
            platform: Platform::Slack,
            attempt: 1,
            delay_secs: 2,
        });

        let event = rx.try_recv().expect("event should be buffered");
        assert!(matches!(
            event.kind,
            AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt: 1,
                delay_secs: 2,
            }
        ));
    }

    #[test]
    fn test_metrics_and_event_bus_coordination() {
        // Verify that metrics store and event bus capture reconnection state
        // correctly together — mirrors what run_socket_mode does per reconnect cycle.
        use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform};

        let store = ChannelMetricsStore::new();
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();

        for attempt in 1..=3u32 {
            let delay_secs = 2u64.pow(attempt.min(5));
            store.record_reconnect("slack", Some(format!("attempt {attempt} failed")));
            bus.emit(AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt,
                delay_secs,
            });
        }

        // Metrics reflects 3 attempts with the last error
        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 3);
        assert_eq!(
            snap["slack"].last_error.as_deref(),
            Some("attempt 3 failed")
        );

        // Event bus has 3 ChannelReconnecting events in order
        for expected_attempt in 1..=3u32 {
            let event = rx.try_recv().expect("event should be buffered");
            match event.kind {
                AppEventKind::ChannelReconnecting { attempt, .. } => {
                    assert_eq!(attempt, expected_attempt);
                }
                _ => panic!("expected ChannelReconnecting event"),
            }
        }

        // After connect: error cleared, uptime set, count remains cumulative
        store.set_connected("slack");
        let snap = store.snapshot();
        assert!(snap["slack"].last_error.is_none());
        assert!(snap["slack"].uptime_secs.is_some());
        assert_eq!(snap["slack"].reconnect_count, 3);
    }
}
