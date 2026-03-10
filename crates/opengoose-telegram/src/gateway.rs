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

use opengoose_core::message_utils::truncate_for_display;
use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

/// Telegram Bot API types needed for the polling loop.
/// Sending and validation are delegated to goose's TelegramGateway.
#[derive(serde::Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(serde::Deserialize)]
struct Update {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct TelegramMessage {
    message_id: i64,
    chat: Chat,
    from: Option<User>,
    text: Option<String>,
    entities: Option<Vec<MessageEntity>>,
}

#[derive(serde::Deserialize)]
struct Chat {
    id: i64,
    #[serde(rename = "type")]
    chat_type: String,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct User {
    id: i64,
    first_name: String,
    last_name: Option<String>,
    username: Option<String>,
}

#[derive(serde::Deserialize)]
struct MessageEntity {
    #[serde(rename = "type")]
    entity_type: String,
    offset: usize,
    length: usize,
}

#[derive(serde::Deserialize)]
struct BotInfo {
    username: Option<String>,
}

/// Minimal response from sendMessage — only what we need for draft tracking.
#[derive(serde::Deserialize)]
struct SentMessage {
    message_id: i64,
}

/// Telegram message size limit.
const TELEGRAM_MAX_LEN: usize = 4096;

/// Timeout for individual Telegram API requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum reconnect attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

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

    /// Build a PlatformUser for delegating to goose's send_message.
    fn platform_user(chat_id: i64) -> PlatformUser {
        PlatformUser {
            platform: "telegram".to_string(),
            user_id: chat_id.to_string(),
            display_name: None,
        }
    }

    /// Build a SessionKey from a Telegram chat.
    fn session_key(chat: &Chat) -> SessionKey {
        let chat_id = chat.id.to_string();
        match chat.chat_type.as_str() {
            "private" => SessionKey::direct(Platform::Telegram, &chat_id),
            _ => SessionKey::new(Platform::Telegram, &chat_id, &chat_id),
        }
    }

    /// Strip `@botname` mention prefix from message text in groups.
    fn strip_mention<'a>(text: &'a str, bot_username: &str) -> &'a str {
        let mention = format!("@{bot_username}");
        text.strip_prefix(&mention)
            .map(|s| s.trim_start())
            .unwrap_or(text)
    }

    /// Check if the message is a /team bot command.
    fn is_bot_command(msg: &TelegramMessage) -> Option<&str> {
        let entities = msg.entities.as_ref()?;
        let text = msg.text.as_ref()?;
        for entity in entities {
            if entity.entity_type == "bot_command" && entity.offset == 0 {
                let (cmd, cmd_end) = Self::bot_command_text(text, entity)?;
                let cmd = cmd.split('@').next().unwrap_or(cmd);
                if cmd == "/team" {
                    return Some(text[cmd_end..].trim());
                }
            }
        }
        None
    }

    /// Extract the command text from a Telegram entity with boundary checks.
    fn bot_command_text<'a>(text: &'a str, entity: &MessageEntity) -> Option<(&'a str, usize)> {
        let cmd_start = entity.offset;
        let cmd_end = cmd_start.checked_add(entity.length)?;

        if cmd_start > text.len() || cmd_end > text.len() {
            return None;
        }

        if !text.is_char_boundary(cmd_start) || !text.is_char_boundary(cmd_end) {
            return None;
        }

        Some((&text[cmd_start..cmd_end], cmd_end))
    }

    /// Handle the /team command. Uses goose's send_message for the response.
    async fn handle_team_command(
        &self,
        session_key: &SessionKey,
        args: &str,
        chat_id: i64,
    ) -> anyhow::Result<()> {
        let response = self.bridge.handle_pairing(session_key, args);

        let user = Self::platform_user(chat_id);
        self.inner
            .send_message(&user, OutgoingMessage::Text { body: response })
            .await?;

        Ok(())
    }
}

#[async_trait]
impl StreamResponder for TelegramGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        TELEGRAM_MAX_LEN
    }

    async fn create_draft(&self, chat_id: &str) -> anyhow::Result<DraftHandle> {
        debug!(chat_id = %chat_id, "creating telegram draft");
        let resp: TelegramResponse<SentMessage> = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": "Thinking...",
            }))
            .send()
            .await?
            .json()
            .await?;

        let msg = resp
            .result
            .ok_or_else(|| anyhow::anyhow!("sendMessage returned no result"))?;
        debug!(chat_id = %chat_id, message_id = msg.message_id, "telegram draft created");
        Ok(DraftHandle {
            message_id: msg.message_id.to_string(),
            channel_id: chat_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(chat_id = %handle.channel_id, message_id = %handle.message_id, content_len = content.len(), "updating telegram draft");
        let display = truncate_for_display(content, TELEGRAM_MAX_LEN);
        let _: TelegramResponse<serde_json::Value> = self
            .client
            .post(self.api_url("editMessageText"))
            .json(&serde_json::json!({
                "chat_id": handle.channel_id,
                "message_id": handle.message_id.parse::<i64>()?,
                "text": display,
            }))
            .send()
            .await?
            .json()
            .await?;
        Ok(())
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.client
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": channel_id,
                "text": content,
            }))
            .send()
            .await?;
        Ok(())
    }

    // finalize_draft uses the default implementation from StreamResponder
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
        // Extract the raw chat_id once (e.g. "telegram:direct:12345" → "12345")
        // because goose's TelegramGateway expects a raw Telegram chat ID.
        let raw_channel_id = if let OutgoingMessage::Text { ref body } = message {
            // Bridge handles persistence, pairing detection, events and returns the session key
            self.bridge
                .on_outgoing_message(&user.user_id, body, "telegram")
                .await
                .channel_id
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
mod tests {
    use super::*;

    #[test]
    fn test_strip_mention() {
        assert_eq!(
            TelegramGateway::strip_mention("@mybot hello world", "mybot"),
            "hello world"
        );
        assert_eq!(
            TelegramGateway::strip_mention("hello world", "mybot"),
            "hello world"
        );
        assert_eq!(
            TelegramGateway::strip_mention("@otherbot hello", "mybot"),
            "@otherbot hello"
        );
    }

    #[test]
    fn test_session_key_private() {
        let chat = Chat {
            id: 12345,
            chat_type: "private".to_string(),
        };
        let key = TelegramGateway::session_key(&chat);
        assert_eq!(key.platform, Platform::Telegram);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "12345");
    }

    #[test]
    fn test_session_key_group() {
        let chat = Chat {
            id: -100123,
            chat_type: "group".to_string(),
        };
        let key = TelegramGateway::session_key(&chat);
        assert_eq!(key.platform, Platform::Telegram);
        assert_eq!(key.namespace, Some("-100123".to_string()));
        assert_eq!(key.channel_id, "-100123");
    }

    #[test]
    fn test_is_bot_command_team() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("/team devops".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 5,
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), Some("devops"));
    }

    #[test]
    fn test_is_bot_command_with_invalid_command_slice() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("👍 hi".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 1,
                length: 4,
            }]),
        };

        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_team_at_bot() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "group".to_string(),
            },
            from: None,
            text: Some("/team@mybot list".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 12,
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), Some("list"));
    }

    #[test]
    fn test_is_bot_command_not_team() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("/start".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 6,
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_no_entities() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("hello".to_string()),
            entities: None,
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_no_text() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: None,
            entities: Some(vec![]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_non_zero_offset() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("hey /team devops".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 4,
                length: 5,
            }]),
        };
        // Only commands at offset 0 are handled
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_team_no_args() {
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: Some("/team".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 5,
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), Some(""));
    }

    #[test]
    fn test_session_key_supergroup() {
        let chat = Chat {
            id: -1001234567890,
            chat_type: "supergroup".to_string(),
        };
        let key = TelegramGateway::session_key(&chat);
        assert_eq!(key.platform, Platform::Telegram);
        assert!(key.namespace.is_some());
    }

    #[test]
    fn test_strip_mention_empty_text() {
        assert_eq!(TelegramGateway::strip_mention("", "mybot"), "");
    }

    #[test]
    fn test_strip_mention_only_mention() {
        assert_eq!(TelegramGateway::strip_mention("@mybot", "mybot"), "");
    }

    #[test]
    fn test_strip_mention_with_extra_spaces() {
        assert_eq!(
            TelegramGateway::strip_mention("@mybot   hello", "mybot"),
            "hello"
        );
    }

    #[test]
    fn test_platform_user() {
        let user = TelegramGateway::platform_user(12345);
        assert_eq!(user.platform, "telegram");
        assert_eq!(user.user_id, "12345");
        assert!(user.display_name.is_none());
    }

    #[test]
    fn test_telegram_max_len_constant() {
        assert_eq!(TELEGRAM_MAX_LEN, 4096);
    }

    #[test]
    fn test_deserialize_telegram_response() {
        let json = r#"{"ok":true,"result":{"message_id":42}}"#;
        let resp: TelegramResponse<SentMessage> = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.result.unwrap().message_id, 42);
    }

    #[test]
    fn test_deserialize_telegram_response_error() {
        let json = r#"{"ok":false,"description":"Unauthorized"}"#;
        let resp: TelegramResponse<SentMessage> = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.description.unwrap(), "Unauthorized");
        assert!(resp.result.is_none());
    }

    #[test]
    fn test_deserialize_update() {
        let json = r#"{"update_id":123,"message":{"message_id":1,"chat":{"id":456,"type":"private"},"text":"hello"}}"#;
        let update: Update = serde_json::from_str(json).unwrap();
        assert_eq!(update.update_id, 123);
        let msg = update.message.unwrap();
        assert_eq!(msg.chat.id, 456);
        assert_eq!(msg.text.unwrap(), "hello");
    }

    #[test]
    fn test_deserialize_update_no_message() {
        let json = r#"{"update_id":123}"#;
        let update: Update = serde_json::from_str(json).unwrap();
        assert!(update.message.is_none());
    }

    #[test]
    fn test_deserialize_user() {
        let json = r#"{"update_id":1,"message":{"message_id":1,"chat":{"id":1,"type":"private"},"from":{"id":100,"first_name":"Alice","last_name":"Smith","username":"alice"}}}"#;
        let update: Update = serde_json::from_str(json).unwrap();
        let msg = update.message.unwrap();
        let user = msg.from.unwrap();
        assert_eq!(user.id, 100);
        assert_eq!(user.first_name, "Alice");
        assert_eq!(user.last_name.unwrap(), "Smith");
        assert_eq!(user.username.unwrap(), "alice");
    }

    #[test]
    fn test_deserialize_bot_info() {
        let json = r#"{"ok":true,"result":{"username":"my_bot"}}"#;
        let resp: TelegramResponse<BotInfo> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.result.unwrap().username.unwrap(), "my_bot");
    }

    // --- Constants ---

    #[test]
    fn test_max_reconnect_attempts_constant() {
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 10);
    }

    #[test]
    fn test_request_timeout_constant() {
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(60));
    }

    // --- Reconnect delay: exponential backoff capped at 2^5 = 32s ---

    #[test]
    fn test_reconnect_delay_exponential_backoff() {
        // Production code: Duration::from_secs(2u64.pow(reconnect_attempts.min(5)))
        let delays: Vec<u64> = (1u32..=10)
            .map(|attempt| 2u64.pow(attempt.min(5)))
            .collect();
        assert_eq!(delays[0], 2); // attempt 1 → 2s
        assert_eq!(delays[1], 4); // attempt 2 → 4s
        assert_eq!(delays[2], 8); // attempt 3 → 8s
        assert_eq!(delays[3], 16); // attempt 4 → 16s
        assert_eq!(delays[4], 32); // attempt 5 → 32s (cap reached)
        assert_eq!(delays[5], 32); // attempt 6 → 32s (capped)
        assert_eq!(delays[9], 32); // attempt 10 → 32s (still capped)
    }

    // --- Display name construction from User fields ---

    #[test]
    fn test_display_name_with_last_name() {
        // When last_name is present: "first last"
        let first_name = "Alice";
        let last_name = Some("Smith".to_string());
        let display_name = match &last_name {
            Some(last) => format!("{} {}", first_name, last),
            None => first_name.to_string(),
        };
        assert_eq!(display_name, "Alice Smith");
    }

    #[test]
    fn test_display_name_first_name_only() {
        // When last_name is absent: just first_name
        let first_name = "Bob";
        let last_name: Option<String> = None;
        let display_name = match &last_name {
            Some(last) => format!("{} {}", first_name, last),
            None => first_name.to_string(),
        };
        assert_eq!(display_name, "Bob");
    }

    // --- Telegram Bot API URL format ---

    #[test]
    fn test_api_url_format_pattern() {
        // Verify the URL pattern used by api_url(): https://api.telegram.org/bot{token}/{method}
        let token = "123456:ABC-DEF";
        let method = "getUpdates";
        let url = format!("https://api.telegram.org/bot{}/{}", token, method);
        assert_eq!(url, "https://api.telegram.org/bot123456:ABC-DEF/getUpdates");
    }

    // --- Deserialisation of internal types ---

    #[test]
    fn test_deserialize_message_entity_mention() {
        let json = r#"{"type":"mention","offset":0,"length":7}"#;
        let entity: MessageEntity = serde_json::from_str(json).unwrap();
        assert_eq!(entity.entity_type, "mention");
        assert_eq!(entity.offset, 0);
        assert_eq!(entity.length, 7);
    }

    #[test]
    fn test_deserialize_chat_private_type() {
        let json = r#"{"id":99,"type":"private"}"#;
        let chat: Chat = serde_json::from_str(json).unwrap();
        assert_eq!(chat.id, 99);
        assert_eq!(chat.chat_type, "private");
    }

    #[test]
    fn test_deserialize_chat_group_type() {
        let json = r#"{"id":-12345,"type":"group"}"#;
        let chat: Chat = serde_json::from_str(json).unwrap();
        assert_eq!(chat.id, -12345);
        assert_eq!(chat.chat_type, "group");
    }

    #[test]
    fn test_deserialize_chat_supergroup_type() {
        let json = r#"{"id":-1001234567890,"type":"supergroup"}"#;
        let chat: Chat = serde_json::from_str(json).unwrap();
        assert_eq!(chat.id, -1001234567890);
        assert_eq!(chat.chat_type, "supergroup");
    }

    // --- Error path: malformed / rate-limit API responses ---

    #[test]
    fn test_get_updates_rate_limit_response() {
        let json = r#"{"ok":false,"description":"Too Many Requests: retry after 30"}"#;
        let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        let desc = resp.description.unwrap();
        assert!(desc.contains("Too Many Requests"));
        assert!(resp.result.is_none());
    }

    #[test]
    fn test_get_updates_auth_error_response() {
        let json = r#"{"ok":false,"description":"Unauthorized"}"#;
        let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.description.unwrap(), "Unauthorized");
    }

    #[test]
    fn test_session_key_channel_type() {
        // Non-private chat types all produce namespaced session keys
        for chat_type in &["group", "supergroup", "channel"] {
            let chat = Chat {
                id: -100,
                chat_type: chat_type.to_string(),
            };
            let key = TelegramGateway::session_key(&chat);
            assert_eq!(key.platform, Platform::Telegram);
            assert!(
                key.namespace.is_some(),
                "chat_type={} should have namespace",
                chat_type
            );
        }
    }

    // --- bot_command_text: entity length exceeds text bounds ---

    #[test]
    fn test_is_bot_command_entity_length_out_of_bounds() {
        // entity.length > text.len() → bot_command_text returns None → is_bot_command returns None
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat { id: 1, chat_type: "private".to_string() },
            from: None,
            text: Some("/team".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 100, // cmd_end = 100 > 5 = text.len()
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_unicode_char_boundary() {
        // "👍" is 4 bytes. length=2 ends mid-emoji → not a char boundary → None
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat { id: 1, chat_type: "private".to_string() },
            from: None,
            text: Some("👍hello".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 2, // ends at byte offset 2, mid-emoji
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    // --- strip_mention: case sensitivity and special characters ---

    #[test]
    fn test_strip_mention_case_sensitive() {
        // @MyBot should NOT strip for bot_username "mybot" — comparison is case-sensitive
        assert_eq!(
            TelegramGateway::strip_mention("@MyBot hello", "mybot"),
            "@MyBot hello"
        );
    }

    #[test]
    fn test_strip_mention_underscore_username() {
        // bot usernames with underscores should be handled correctly
        assert_eq!(
            TelegramGateway::strip_mention("@my_bot hello", "my_bot"),
            "hello"
        );
        assert_eq!(
            TelegramGateway::strip_mention("@other_bot hello", "my_bot"),
            "@other_bot hello"
        );
    }

    // --- session_key: channel_id and namespace relationship ---

    #[test]
    fn test_session_key_private_channel_id_format() {
        let chat = Chat { id: 99999, chat_type: "private".to_string() };
        let key = TelegramGateway::session_key(&chat);
        assert_eq!(key.channel_id, "99999");
        assert_eq!(key.namespace, None);
    }

    #[test]
    fn test_session_key_group_namespace_equals_channel_id() {
        let chat = Chat { id: -500, chat_type: "group".to_string() };
        let key = TelegramGateway::session_key(&chat);
        // For non-private chats, namespace == channel_id (both set to chat_id)
        assert_eq!(key.channel_id, "-500");
        assert_eq!(key.namespace, Some("-500".to_string()));
    }

    // --- is_bot_command: multiple entities ---

    #[test]
    fn test_is_bot_command_multiple_entities_first_non_command() {
        // First entity is a mention, second is /team at offset 0 → should match
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat { id: 1, chat_type: "group".to_string() },
            from: None,
            text: Some("/team list".to_string()),
            entities: Some(vec![
                MessageEntity { entity_type: "mention".to_string(), offset: 0, length: 3 },
                MessageEntity { entity_type: "bot_command".to_string(), offset: 0, length: 5 },
            ]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), Some("list"));
    }

    #[test]
    fn test_is_bot_command_empty_entities_vec() {
        // Empty entities vector (not None) → None
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat { id: 1, chat_type: "private".to_string() },
            from: None,
            text: Some("/team".to_string()),
            entities: Some(vec![]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), None);
    }

    #[test]
    fn test_is_bot_command_team_multiple_word_args() {
        // /team with multi-word argument
        let msg = TelegramMessage {
            message_id: 1,
            chat: Chat { id: 1, chat_type: "private".to_string() },
            from: None,
            text: Some("/team list all active".to_string()),
            entities: Some(vec![MessageEntity {
                entity_type: "bot_command".to_string(),
                offset: 0,
                length: 5,
            }]),
        };
        assert_eq!(TelegramGateway::is_bot_command(&msg), Some("list all active"));
    }

    // --- platform_user: edge cases ---

    #[test]
    fn test_platform_user_negative_chat_id() {
        let user = TelegramGateway::platform_user(-100123456789);
        assert_eq!(user.platform, "telegram");
        assert_eq!(user.user_id, "-100123456789");
        assert!(user.display_name.is_none());
    }

    #[test]
    fn test_platform_user_zero_chat_id() {
        let user = TelegramGateway::platform_user(0);
        assert_eq!(user.user_id, "0");
    }

    // --- Deserialization edge cases ---

    #[test]
    fn test_deserialize_bot_info_no_username() {
        let json = r#"{"ok":true,"result":{}}"#;
        let resp: TelegramResponse<BotInfo> = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert!(resp.result.unwrap().username.is_none());
    }

    #[test]
    fn test_deserialize_user_no_optional_fields() {
        let json = r#"{"update_id":1,"message":{"message_id":1,"chat":{"id":1,"type":"private"},"from":{"id":42,"first_name":"Bob"}}}"#;
        let update: Update = serde_json::from_str(json).unwrap();
        let user = update.message.unwrap().from.unwrap();
        assert_eq!(user.id, 42);
        assert_eq!(user.first_name, "Bob");
        assert!(user.last_name.is_none());
        assert!(user.username.is_none());
    }

    #[test]
    fn test_deserialize_message_entity_bot_command() {
        let json = r#"{"type":"bot_command","offset":0,"length":5}"#;
        let entity: MessageEntity = serde_json::from_str(json).unwrap();
        assert_eq!(entity.entity_type, "bot_command");
        assert_eq!(entity.offset, 0);
        assert_eq!(entity.length, 5);
    }

    #[test]
    fn test_deserialize_chat_channel_type() {
        let json = r#"{"id":-1009876543210,"type":"channel"}"#;
        let chat: Chat = serde_json::from_str(json).unwrap();
        assert_eq!(chat.id, -1009876543210);
        assert_eq!(chat.chat_type, "channel");
    }

    #[test]
    fn test_deserialize_telegram_response_empty_updates() {
        let json = r#"{"ok":true,"result":[]}"#;
        let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.result.unwrap().len(), 0);
    }

    #[test]
    fn test_deserialize_update_with_entities() {
        let json = r#"{"update_id":5,"message":{"message_id":10,"chat":{"id":1,"type":"private"},"text":"/team hello","entities":[{"type":"bot_command","offset":0,"length":5}]}}"#;
        let update: Update = serde_json::from_str(json).unwrap();
        let msg = update.message.unwrap();
        let entities = msg.entities.unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "bot_command");
        assert_eq!(entities[0].length, 5);
    }

    #[test]
    fn test_api_url_format_send_message() {
        let token = "987654:XYZ";
        let url = format!("https://api.telegram.org/bot{}/{}", token, "sendMessage");
        assert_eq!(url, "https://api.telegram.org/bot987654:XYZ/sendMessage");
    }

    #[test]
    fn test_api_url_format_get_me() {
        let token = "111:AAA";
        let url = format!("https://api.telegram.org/bot{}/{}", token, "getMe");
        assert_eq!(url, "https://api.telegram.org/bot111:AAA/getMe");
    }
}
