//! Telegram gateway implementation: long-polling getUpdates loop.
//!
//! [`TelegramGateway`] implements the `Gateway` trait. It polls the Telegram
//! Bot API`s `getUpdates` endpoint with exponential back-off and delivers
//! replies via `sendMessage`. Supports both private chats and group channels.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use goose::gateway::handler::GatewayHandler;
use goose::gateway::telegram::TelegramGateway as GooseTelegramGateway;
use goose::gateway::{Gateway, GatewayConfig, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::GatewayBridge;
use opengoose_types::{EventBus, Platform, SessionKey};

mod commands;
mod delivery;
mod polling;
mod relay;
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
    api_base_url: String,
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
        Self::build(
            bot_token.into(),
            bridge,
            event_bus,
            "https://api.telegram.org".to_string(),
        )
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.api_base_url, self.bot_token, method)
    }

    fn build(
        bot_token: String,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
        api_base_url: String,
    ) -> anyhow::Result<Self> {
        // Construct goose's TelegramGateway for sending/validation.
        let config = GatewayConfig {
            gateway_type: "telegram".to_string(),
            platform_config: serde_json::json!({ "bot_token": &bot_token }),
            max_sessions: 100,
        };
        let inner = GooseTelegramGateway::new(&config)
            .map_err(|e| anyhow::anyhow!("failed to create TelegramGateway: {e}"))?;

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build reqwest client: {e}"))?;

        Ok(Self {
            bot_token,
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            client,
            inner,
            bridge,
            event_bus,
        })
    }

    #[cfg(test)]
    fn with_api_base_url(
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
        api_base_url: impl Into<String>,
    ) -> anyhow::Result<Self> {
        Self::build(bot_token.into(), bridge, event_bus, api_base_url.into())
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
        self.run_polling_loop(cancel).await
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        self.deliver_outgoing_message(user, message).await
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
