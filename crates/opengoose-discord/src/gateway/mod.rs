//! Discord gateway implementation: Twilight WebSocket event loop.
//!
//! [`DiscordGateway`] implements the `Gateway` trait using the Twilight
//! library. It connects to the Discord Gateway WebSocket, filters events
//! to direct messages and configured channels, and posts replies via the
//! Discord REST API. Tracks processed message IDs to avoid duplicates.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tracing::{debug, error, info, warn};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client as HttpClient;
use twilight_model::id::Id;
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker, MessageMarker};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::message_utils::truncate_for_display;
use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform, SessionKey};

mod helpers;
use helpers::{handle_interaction, handle_message, register_slash_commands, split_discord_chunks};

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
    active_drafts: DashMap<String, DraftHandle>,
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
            active_drafts: DashMap::new(),
        }
    }

    /// Send a text message to a Discord channel, splitting if needed.
    async fn send_to_channel(&self, channel_id: Id<ChannelMarker>, body: &str) {
        let chunks = split_discord_chunks(body);
        debug!(channel_id = %channel_id, chunks = chunks.len(), body_len = body.len(), "sending discord message");
        for chunk in chunks {
            if let Err(e) = self.http.create_message(channel_id).content(chunk).await {
                error!(%e, channel_id = %channel_id, "failed to send discord message");
            }
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
        // Register handler with bridge for team orchestration
        self.bridge.on_start(handler).await;

        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, self.token.clone(), intents);

        info!("discord gateway starting");

        // Track application_id for slash commands (set on Ready)
        let mut application_id: Option<Id<ApplicationMarker>> = None;

        // Deduplication cache: tracks recently-processed message IDs to
        // prevent double-handling during Discord WebSocket reconnects/replays.
        let mut seen: HashSet<Id<MessageMarker>> = HashSet::new();
        let mut seen_order: Vec<Id<MessageMarker>> = Vec::new();

        // Discord event loop
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("discord gateway shutting down");
                    self.event_bus.emit(AppEventKind::ChannelDisconnected {
                        platform: Platform::Discord,
                        reason: "shutdown".into(),
                    });
                    break;
                }
                event = shard.next_event(EventTypeFlags::all()) => {
                    match event {
                        Some(Ok(event)) => match event {
                            Event::MessageCreate(msg) => {
                                if !seen.insert(msg.id) {
                                    warn!(msg_id = %msg.id, "duplicate MessageCreate ignored");
                                    continue;
                                }
                                seen_order.push(msg.id);
                                if seen_order.len() > SEEN_MESSAGES_CAPACITY {
                                    let evicted = seen_order.remove(0);
                                    seen.remove(&evicted);
                                }
                                handle_message(&self.bridge, self, &msg.0).await;
                            }
                            Event::Ready(ready) => {
                                let app_id = ready.application.id;
                                application_id = Some(app_id);
                                info!(?app_id, "discord gateway connected");
                                self.event_bus.emit(AppEventKind::ChannelReady {
                                    platform: Platform::Discord,
                                });
                                self.metrics.set_connected("discord");

                                // Register /team slash command
                                if let Err(e) = register_slash_commands(&self.http, app_id).await {
                                    error!(%e, "failed to register slash commands");
                                }
                            }
                            Event::InteractionCreate(interaction) => {
                                if let Some(app_id) = application_id {
                                    handle_interaction(
                                        &self.http,
                                        app_id,
                                        &self.bridge,
                                        &interaction.0,
                                    )
                                    .await;
                                }
                            }
                            _ => {}
                        },
                        Some(Err(e)) => {
                            warn!(%e, "discord gateway error, twilight will auto-reconnect");
                            self.metrics.record_reconnect("discord", Some(e.to_string()));
                            self.event_bus.emit(AppEventKind::ChannelReconnecting {
                                platform: Platform::Discord,
                                // twilight manages attempt tracking internally; we report 0
                                // to indicate an auto-reconnect without a specific attempt count.
                                attempt: 0,
                                delay_secs: 0,
                            });
                        }
                        None => {
                            error!("discord shard closed -- check bot token and privileged intents");
                            let reason = "Discord connection closed. Verify your bot token and that MESSAGE_CONTENT intent is enabled in the Developer Portal.".to_string();
                            self.event_bus.emit(AppEventKind::ChannelDisconnected {
                                platform: Platform::Discord,
                                reason: reason.clone(),
                            });
                            self.event_bus.emit(AppEventKind::Error {
                                context: "discord".into(),
                                message: reason,
                            });
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        match message {
            OutgoingMessage::Typing => {
                // Post a placeholder message immediately so the user sees
                // activity while Goose processes.  Only create one draft per
                // user; subsequent Typing events (between tool calls) are no-ops.
                debug!(user_id = %user.user_id, "discord outgoing typing indicator");
                let session_key = SessionKey::from_stable_id(&user.user_id);
                let channel_id_str = session_key.channel_id;

                let already_has_draft = self
                    .active_drafts
                    .contains_key(&user.user_id);

                if !already_has_draft {
                    match self.create_draft(&channel_id_str).await {
                        Ok(handle) => {
                            self.active_drafts
                                .insert(user.user_id.clone(), handle);
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "failed to create typing draft");
                        }
                    }
                }
            }
            OutgoingMessage::Text { body } => {
                debug!(user_id = %user.user_id, body_len = body.len(), "discord outgoing text message");
                // Bridge handles persistence, pairing detection, events, and channel routing.
                let channel_id = self
                    .bridge
                    .route_outgoing_text(&user.user_id, &body, "discord")
                    .await;

                // If a draft placeholder exists, replace it in-place; otherwise
                // send a new message (pairing flow, error messages, etc.).
                let draft = self
                    .active_drafts
                    .remove(&user.user_id);

                match draft {
                    Some((_, handle)) => {
                        if let Err(e) = self.finalize_draft(&handle, &body).await {
                            tracing::warn!(error = %e, "failed to finalize draft; falling back to new message");
                            let channel_id = match channel_id.parse::<u64>() {
                                Ok(id) => Id::<ChannelMarker>::new(id),
                                Err(_) => return Ok(()),
                            };
                            self.send_to_channel(channel_id, &body).await;
                        }
                    }
                    None => {
                        let channel_id = match channel_id.parse::<u64>() {
                            Ok(id) => Id::<ChannelMarker>::new(id),
                            Err(_) => {
                                warn!(channel_id = %channel_id, "invalid channel id");
                                return Ok(());
                            }
                        };
                        self.send_to_channel(channel_id, &body).await;
                    }
                }
            }
        }
        Ok(())
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
        debug!(channel_id = %channel_id, "creating discord draft");
        let ch_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        let msg = self
            .http
            .create_message(ch_id)
            .content("Thinking...")
            .await?
            .model()
            .await?;
        debug!(channel_id = %channel_id, message_id = %msg.id, "discord draft created");
        Ok(DraftHandle {
            message_id: msg.id.to_string(),
            channel_id: channel_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(channel_id = %handle.channel_id, message_id = %handle.message_id, content_len = content.len(), "updating discord draft");
        let ch_id = Id::<ChannelMarker>::new(handle.channel_id.parse()?);
        let msg_id = Id::new(handle.message_id.parse()?);
        let display = truncate_for_display(content, DISCORD_MAX_LEN);
        self.http
            .update_message(ch_id, msg_id)
            .content(Some(display))
            .await?;
        Ok(())
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        let ch_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        self.http.create_message(ch_id).content(content).await?;
        Ok(())
    }

    // finalize_draft uses the default implementation from StreamResponder
}
