//! Slack Socket Mode gateway implementation.

mod dispatch;
mod envelope;
mod messages;
mod socket;
mod types;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform};

/// Safety split at 4000 chars for readability (Slack allows ~40k).
pub(crate) const SLACK_MAX_LEN: usize = 4000;

/// Maximum number of consecutive reconnect attempts before giving up.
pub(crate) const MAX_RECONNECT_ATTEMPTS: u32 = 10;

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
            let channel_id = self
                .bridge
                .route_outgoing_text(&user.user_id, &body, "slack")
                .await;

            if let Err(e) = self.post_message(&channel_id, &body).await {
                error!(%e, "failed to send slack message");
            }
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        self.get_bot_user_id().await.map(|_| ())
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
        self.create_draft_message(channel).await
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        self.update_draft_message(handle, content).await
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.post_message(channel_id, content).await
    }

    // finalize_draft uses the default implementation from StreamResponder
}
