//! Discord gateway implementation: Twilight WebSocket event loop.
//!
//! [`DiscordGateway`] implements the `Gateway` trait using the Twilight
//! library. It connects to the Discord Gateway WebSocket, filters events
//! to direct messages and configured channels, and posts replies via the
//! Discord REST API. Tracks processed message IDs to avoid duplicates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use twilight_http::Client as HttpClient;

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{ChannelMetricsStore, EventBus};

mod drafts;
mod helpers;
mod lifecycle;
mod outgoing;

#[cfg(test)]
mod tests;

/// Discord enforces a 2000-character limit per message.
pub(crate) const DISCORD_MAX_LEN: usize = 2000;

/// Maximum number of recently-processed message IDs to keep in memory.
/// Prevents unbounded growth while covering any realistic replay window.
pub(crate) const SEEN_MESSAGES_CAPACITY: usize = 256;

/// Discord channel gateway implementing the goose `Gateway` trait.
///
/// Combines the old `DiscordAdapter` + `OpenGooseGateway` into a single struct.
/// Uses `GatewayBridge` for shared orchestration (team intercept, persistence, pairing).
///
/// **Draft-based streaming**: When Goose sends a `Typing` indicator, a
/// placeholder "thinking..." message is immediately posted in the channel.
/// When the final `Text` reply arrives it replaces that placeholder in-place,
/// giving users instant visual feedback without waiting for the full response.
pub struct DiscordGateway {
    token: String,
    bridge: Arc<GatewayBridge>,
    http: Arc<HttpClient>,
    event_bus: EventBus,
    metrics: ChannelMetricsStore,
    /// Active placeholder messages keyed by `user_id`.
    /// A `DraftHandle` is inserted when `Typing` is received and removed
    /// (then finalized) when the `Text` response arrives.
    active_drafts: Mutex<HashMap<String, DraftHandle>>,
}

impl DiscordGateway {
    pub fn new(token: impl Into<String>, bridge: Arc<GatewayBridge>, event_bus: EventBus) -> Self {
        Self::with_metrics(token, bridge, event_bus, ChannelMetricsStore::new())
    }

    pub fn with_metrics(
        token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
        metrics: ChannelMetricsStore,
    ) -> Self {
        let token = token.into();
        let http = Arc::new(HttpClient::new(token.clone()));
        Self {
            token,
            bridge,
            http,
            event_bus,
            metrics,
            active_drafts: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Gateway for DiscordGateway {
    fn gateway_type(&self) -> &str {
        "discord"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        self.bridge.on_start(handler).await;
        self.run_gateway_loop(cancel).await
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        self.send_outgoing_message(user, message).await
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "discord".into())])
    }
}

#[async_trait]
impl StreamResponder for DiscordGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        DISCORD_MAX_LEN
    }

    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
        self.create_draft_message(channel_id).await
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        self.update_draft_message(handle, content).await
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.send_draft_overflow(channel_id, content).await
    }

    // finalize_draft uses the default implementation from StreamResponder
}
