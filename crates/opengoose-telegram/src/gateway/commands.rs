//! Bot command parsing and `/team` command handler for Telegram.

use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};

use super::TelegramGateway;
use super::types::{MessageEntity, TelegramMessage};

impl TelegramGateway {
    /// Strip `@botname` mention prefix from message text in groups.
    pub(crate) fn strip_mention<'a>(text: &'a str, bot_username: &str) -> &'a str {
        let mention = format!("@{bot_username}");
        text.strip_prefix(&mention)
            .map(|s| s.trim_start())
            .unwrap_or(text)
    }

    /// Check if the message is a /team bot command.
    pub(crate) fn is_bot_command(msg: &TelegramMessage) -> Option<&str> {
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
    pub(crate) async fn handle_team_command(
        &self,
        session_key: &opengoose_types::SessionKey,
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

    /// Build a PlatformUser for delegating to goose's send_message.
    pub(crate) fn platform_user(chat_id: i64) -> PlatformUser {
        PlatformUser {
            platform: "telegram".to_string(),
            user_id: chat_id.to_string(),
            display_name: None,
        }
    }
}
