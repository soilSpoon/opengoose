//! Telegram gateway implementation: long-polling getUpdates loop.
//!
//! [`TelegramGateway`] implements the `Gateway` trait. It polls the Telegram
//! Bot API`s `getUpdates` endpoint with exponential back-off and delivers
//! replies via `sendMessage`. Supports both private chats and group channels.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{debug, error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::telegram::TelegramGateway as GooseTelegramGateway;
use goose::gateway::{Gateway, GatewayConfig, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::{GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

mod commands;
mod streaming;
mod types;
pub(crate) use types::*;

/// Telegram message size limit.
pub(crate) const TELEGRAM_MAX_LEN: usize = 4096;

/// Timeout for individual Telegram API requests.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum reconnect attempts before giving up.
pub(crate) const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Telegram channel gateway implementing the goose `Gateway` trait.
///
/// Wraps goose's `TelegramGateway` for message sending and config validation,
/// adding opengoose-specific concerns: team orchestration via `GatewayBridge`,
/// `/team` commands, `@botname` mention stripping, and event bus integration.
///
/// The polling loop (getUpdates) is implemented here because we need to
/// intercept messages before they reach the goose handler.
pub struct TelegramGateway {
    /// Used for the polling loop (getUpdates) and bot username lookup.
    bot_token: String,
    client: reqwest::Client,
    /// Goose's TelegramGateway handles send_message and validate_config.
    inner: GooseTelegramGateway,
    bridge: Arc<GatewayBridge>,
    event_bus: EventBus,
}

impl TelegramGateway {
    pub fn new(
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> anyhow::Result<Self> {
        let token = bot_token.into();

        // Construct goose's TelegramGateway for sending/validation.
        let config = GatewayConfig {
            gateway_type: "telegram".to_string(),
            platform_config: serde_json::json!({ "bot_token": &token }),
            max_sessions: 100,
        };
        let inner = GooseTelegramGateway::new(&config)
            .map_err(|e| anyhow::anyhow!("failed to create TelegramGateway: {e}"))?;

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build reqwest client: {e}"))?;

        Ok(Self {
            bot_token: token,
            client,
            inner,
            bridge,
            event_bus,
        })
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }

    /// Long-poll for updates from Telegram.
    /// This must be implemented here (not delegated) because we intercept
    /// messages for /team commands and bridge routing before goose sees them.
    async fn get_updates(&self, offset: Option<i64>) -> anyhow::Result<Vec<Update>> {
        let mut params = serde_json::json!({ "timeout": 30 });
        if let Some(off) = offset {
            params["offset"] = serde_json::json!(off);
        }

        let resp: TelegramResponse<Vec<Update>> = self
            .client
            .post(self.api_url("getUpdates"))
            .json(&params)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "getUpdates failed: {}",
                resp.description.unwrap_or_default()
            );
        }

        Ok(resp.result.unwrap_or_default())
    }

    /// Get the bot's username for mention stripping.
    async fn get_bot_username(&self) -> Option<String> {
        let resp: TelegramResponse<BotInfo> = self
            .client
            .post(self.api_url("getMe"))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        resp.result.and_then(|b| b.username)
    }

    /// Build a SessionKey from a Telegram chat.
    fn session_key(chat: &Chat) -> SessionKey {
        let chat_id = chat.id.to_string();
        match chat.chat_type.as_str() {
            "private" => SessionKey::direct(Platform::Telegram, &chat_id),
            _ => SessionKey::new(Platform::Telegram, &chat_id, &chat_id),
        }
    }
}

#[async_trait]
impl Gateway for TelegramGateway {
    fn gateway_type(&self) -> &str {
        "telegram"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        self.bridge.on_start(handler).await;

        let bot_username = self.get_bot_username().await.unwrap_or_default();
        info!(bot_username = %bot_username, "telegram gateway starting");

        let mut offset: Option<i64> = None;
        let mut ready_emitted = false;
        let mut reconnect_attempts: u32 = 0;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("telegram gateway shutting down");
                    self.event_bus.emit(AppEventKind::ChannelDisconnected {
                        platform: Platform::Telegram,
                        reason: "shutdown".into(),
                    });
                    break;
                }
                result = self.get_updates(offset) => {
                    match result {
                        Ok(updates) => {
                            reconnect_attempts = 0;
                            // Emit ready only after first successful poll
                            if !ready_emitted {
                                info!("telegram gateway connected");
                                self.event_bus.emit(AppEventKind::ChannelReady {
                                    platform: Platform::Telegram,
                                });
                                ready_emitted = true;
                            }
                            for update in updates {
                                offset = Some(update.update_id + 1);

                                let Some(msg) = update.message else {
                                    continue;
                                };

                                // Check for /team command
                                if let Some(args) = Self::is_bot_command(&msg) {
                                    let session_key = Self::session_key(&msg.chat);
                                    if let Err(e) = self.handle_team_command(&session_key, args, msg.chat.id).await {
                                        error!(%e, "failed to handle /team command");
                                    }
                                    continue;
                                }

                                let Some(text) = msg.text.as_deref() else {
                                    continue;
                                };

                                // Strip @botname mention in groups
                                let text = if msg.chat.chat_type != "private" && !bot_username.is_empty() {
                                    Self::strip_mention(text, &bot_username)
                                } else {
                                    text
                                };

                                let text = text.trim();
                                if text.is_empty() {
                                    continue;
                                }

                                let session_key = Self::session_key(&msg.chat);
                                let display_name = msg.from.as_ref().map(|u| {
                                    match &u.last_name {
                                        Some(last) => format!("{} {}", u.first_name, last),
                                        None => u.first_name.clone(),
                                    }
                                });

                                debug!(
                                    chat_id = msg.chat.id,
                                    chat_type = %msg.chat.chat_type,
                                    text_len = text.len(),
                                    "relaying telegram message to engine"
                                );

                                // Send typing indicator via goose's gateway
                                let user = Self::platform_user(msg.chat.id);
                                let _ = self.inner.send_message(&user, OutgoingMessage::Typing).await;

                                let chat_id_str = msg.chat.id.to_string();
                                if let Err(e) = self.bridge.relay_and_drive_stream(
                                    &session_key,
                                    display_name,
                                    text,
                                    self as &dyn StreamResponder,
                                    &chat_id_str,
                                    opengoose_core::ThrottlePolicy::telegram(),
                                    TELEGRAM_MAX_LEN,
                                ).await {
                                    // Error event is emitted by bridge; just log here
                                    error!(%e, "failed to relay telegram message");
                                }
                            }
                        }
                        Err(e) => {
                            reconnect_attempts += 1;
                            if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                                let reason = format!("getUpdates failed after {MAX_RECONNECT_ATTEMPTS} attempts: {e}");
                                error!(%e, "telegram gateway giving up after max reconnect attempts");
                                self.event_bus.emit(AppEventKind::ChannelDisconnected {
                                    platform: Platform::Telegram,
                                    reason: reason.clone(),
                                });
                                self.event_bus.emit(AppEventKind::Error {
                                    context: "telegram".into(),
                                    message: reason,
                                });
                                break;
                            }
                            let delay = Duration::from_secs(2u64.pow(reconnect_attempts.min(5)));
                            warn!(%e, attempt = reconnect_attempts, ?delay, "telegram getUpdates error, retrying...");
                            tokio::select! {
                                _ = cancel.cancelled() => {
                                    info!("telegram gateway shutting down during reconnect");
                                    self.event_bus.emit(AppEventKind::ChannelDisconnected {
                                        platform: Platform::Telegram,
                                        reason: "shutdown".into(),
                                    });
                                    break;
                                }
                                _ = tokio::time::sleep(delay) => {}
                            }
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
        // Extract the raw chat_id once (e.g. "telegram:direct:12345" -> "12345")
        // because goose's TelegramGateway expects a raw Telegram chat ID.
        let raw_channel_id = if let OutgoingMessage::Text { ref body } = message {
            // Bridge handles persistence, pairing detection, events, and channel routing.
            self.bridge
                .route_outgoing_text(&user.user_id, body, "telegram")
                .await
        } else {
            SessionKey::from_stable_id(&user.user_id).channel_id
        };

        let raw_user = PlatformUser {
            platform: user.platform.clone(),
            user_id: raw_channel_id,
            display_name: user.display_name.clone(),
        };
        self.inner.send_message(&raw_user, message).await
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        // Delegate to goose's TelegramGateway
        self.inner.validate_config().await
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "telegram".into())])
    }
}

#[cfg(test)]
mod tests;
