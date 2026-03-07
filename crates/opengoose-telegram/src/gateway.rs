use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info, warn};

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
    ) -> Self {
        let token = bot_token.into();

        // Construct goose's TelegramGateway for sending/validation.
        let config = GatewayConfig {
            gateway_type: "telegram".to_string(),
            platform_config: serde_json::json!({ "bot_token": &token }),
            max_sessions: 100,
        };
        let inner = GooseTelegramGateway::new(&config)
            .expect("goose TelegramGateway construction should not fail with a valid token string");

        Self {
            bot_token: token,
            client: reqwest::Client::new(),
            inner,
            bridge,
            event_bus,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{method}", self.bot_token)
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
                let cmd_end = entity.offset + entity.length;
                let cmd = &text[..cmd_end];
                let cmd = cmd.split('@').next().unwrap_or(cmd);
                if cmd == "/team" {
                    return Some(text[cmd_end..].trim());
                }
            }
        }
        None
    }

    /// Handle the /team command. Uses goose's send_message for the response.
    async fn handle_team_command(
        &self,
        session_key: &SessionKey,
        args: &str,
        chat_id: i64,
    ) -> anyhow::Result<()> {
        let response = self.bridge.engine().handle_team_command(session_key, args);

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
        Ok(DraftHandle {
            message_id: msg.message_id.to_string(),
            channel_id: chat_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
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
                            // Emit ready only after first successful poll
                            if !ready_emitted {
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
                                let display_name = msg.from.as_ref().map(|u| match &u.last_name {
                                    Some(last) => format!("{} {last}", u.first_name),
                                    None => u.first_name.clone(),
                                });

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
                            warn!(%e, "telegram getUpdates error, retrying...");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
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
        // Bridge handles persistence, pairing detection, events and returns the session key
        if let OutgoingMessage::Text { ref body } = message {
            let session_key = self
                .bridge
                .on_outgoing_message(&user.user_id, body, "telegram")
                .await;

            // Extract the raw chat_id (e.g. "telegram:direct:12345" → "12345")
            // because goose's TelegramGateway expects a raw Telegram chat ID.
            let raw_user = PlatformUser {
                platform: user.platform.clone(),
                user_id: session_key.channel_id,
                display_name: user.display_name.clone(),
            };

            return self.inner.send_message(&raw_user, message).await;
        }

        // Non-text messages (e.g. typing) — delegate with raw chat_id
        let raw_user = PlatformUser {
            platform: user.platform.clone(),
            user_id: SessionKey::from_stable_id(&user.user_id).channel_id,
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
}
