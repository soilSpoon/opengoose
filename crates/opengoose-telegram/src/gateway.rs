use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::GatewayBridge;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

use crate::format::markdown_to_telegram_html;

/// Telegram enforces a 4096-character limit per message.
const TELEGRAM_MAX_LEN: usize = 4096;

/// Telegram Bot API types (minimal, following goose pattern).
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

/// Telegram channel gateway implementing the goose `Gateway` trait.
///
/// Uses reqwest + long-polling (same pattern as goose's TelegramGateway)
/// with `GatewayBridge` for team orchestration.
pub struct TelegramGateway {
    bot_token: String,
    client: reqwest::Client,
    bridge: Arc<GatewayBridge>,
    event_bus: EventBus,
}

impl TelegramGateway {
    pub fn new(
        bot_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            bridge,
            event_bus,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }

    /// Long-poll for updates from Telegram.
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

    /// Send a text message to a Telegram chat.
    async fn send_text(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        let html = markdown_to_telegram_html(text);
        for chunk in split_message(&html, TELEGRAM_MAX_LEN) {
            let params = serde_json::json!({
                "chat_id": chat_id,
                "text": chunk,
                "parse_mode": "HTML",
            });

            let resp: TelegramResponse<serde_json::Value> = self
                .client
                .post(self.api_url("sendMessage"))
                .json(&params)
                .send()
                .await?
                .json()
                .await?;

            if !resp.ok {
                warn!(
                    "sendMessage failed: {}",
                    resp.description.unwrap_or_default()
                );
            }
        }
        Ok(())
    }

    /// Send a typing indicator.
    async fn send_typing(&self, chat_id: i64) {
        let params = serde_json::json!({
            "chat_id": chat_id,
            "action": "typing",
        });
        let _ = self
            .client
            .post(self.api_url("sendChatAction"))
            .json(&params)
            .send()
            .await;
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
            // Groups use chat_id as both namespace and channel
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
                // Handle /team or /team@botname
                let cmd = cmd.split('@').next().unwrap_or(cmd);
                if cmd == "/team" {
                    return Some(text[cmd_end..].trim());
                }
            }
        }
        None
    }

    /// Handle the /team command.
    async fn handle_team_command(
        &self,
        session_key: &SessionKey,
        args: &str,
        chat_id: i64,
    ) -> anyhow::Result<()> {
        let engine = self.bridge.engine();
        let response = match args {
            "" => match engine.active_team_for(session_key) {
                Some(team) => format!("Active team for this chat: <b>{team}</b>"),
                None => "No team active for this chat.".to_string(),
            },
            "off" => {
                engine.clear_active_team(session_key);
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
                            .map(|t| format!("- {t}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            team_name => {
                if engine.team_exists(team_name) {
                    engine.set_active_team(session_key, team_name.to_string());
                    format!("Team <b>{team_name}</b> activated for this chat.")
                } else {
                    let available = engine.list_teams();
                    format!(
                        "Team <code>{team_name}</code> not found. Available teams: {}",
                        if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        }
                    )
                }
            }
        };

        let params = serde_json::json!({
            "chat_id": chat_id,
            "text": response,
            "parse_mode": "HTML",
        });
        let _ = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&params)
            .send()
            .await;

        Ok(())
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

        // Get bot username for mention stripping
        let bot_username = self.get_bot_username().await.unwrap_or_default();
        info!(bot_username = %bot_username, "telegram gateway starting");

        self.event_bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Telegram,
        });

        let mut offset: Option<i64> = None;

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

                                self.send_typing(msg.chat.id).await;

                                match self.bridge.relay_message(&session_key, display_name, text).await {
                                    Ok(Some(response)) => {
                                        // Team handled it — send response directly
                                        if let Err(e) = self.send_text(msg.chat.id, &response).await {
                                            error!(%e, "failed to send team response");
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
                                        error!(%e, "failed to relay telegram message");
                                    }
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
        if let OutgoingMessage::Text { body } = message {
            // Let bridge handle persistence, pairing detection, events
            self.bridge
                .on_outgoing_message(&user.user_id, &body, "telegram")
                .await;

            // Extract chat_id from session key
            let session_key = SessionKey::from_stable_id(&user.user_id);
            let chat_id: i64 = session_key
                .channel_id
                .parse()
                .unwrap_or_else(|_| {
                    warn!(channel_id = %session_key.channel_id, "invalid telegram chat_id");
                    0
                });

            if chat_id != 0 {
                if let Err(e) = self.send_text(chat_id, &body).await {
                    error!(%e, "failed to send telegram message");
                }
            }
        } else {
            // Typing indicator
            let session_key = SessionKey::from_stable_id(&user.user_id);
            if let Ok(chat_id) = session_key.channel_id.parse::<i64>() {
                self.send_typing(chat_id).await;
            }
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        let resp: TelegramResponse<BotInfo> = self
            .client
            .post(self.api_url("getMe"))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "Telegram bot token validation failed: {}",
                resp.description.unwrap_or_default()
            );
        }

        Ok(())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "telegram".into())])
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
    fn test_split_short_message() {
        assert_eq!(split_message("hello", TELEGRAM_MAX_LEN), vec!["hello"]);
    }

    #[test]
    fn test_split_long_message() {
        let msg = "a".repeat(5000);
        let chunks = split_message(&msg, TELEGRAM_MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), TELEGRAM_MAX_LEN);
        assert_eq!(chunks[1].len(), 904);
    }

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
}
