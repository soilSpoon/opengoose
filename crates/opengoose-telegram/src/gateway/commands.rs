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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::types::Chat;

    fn message_with_command(
        text: Option<&str>,
        entity_type: &str,
        offset: usize,
        length: usize,
    ) -> TelegramMessage {
        TelegramMessage {
            chat: Chat {
                id: 1,
                chat_type: "private".to_string(),
            },
            from: None,
            text: text.map(str::to_string),
            entities: Some(vec![MessageEntity {
                entity_type: entity_type.to_string(),
                offset,
                length,
            }]),
        }
    }

    #[test]
    fn strip_mention_only_removes_matching_prefix() {
        assert_eq!(
            TelegramGateway::strip_mention("@my_bot   hello", "my_bot"),
            "hello"
        );
        assert_eq!(
            TelegramGateway::strip_mention("@other_bot hello", "my_bot"),
            "@other_bot hello"
        );
        assert_eq!(
            TelegramGateway::strip_mention("@MyBot hello", "mybot"),
            "@MyBot hello"
        );
    }

    #[test]
    fn bot_command_text_rejects_out_of_bounds_entities() {
        let text = "/team";
        let entity = MessageEntity {
            entity_type: "bot_command".to_string(),
            offset: 0,
            length: 100,
        };

        assert_eq!(TelegramGateway::bot_command_text(text, &entity), None);
    }

    #[test]
    fn bot_command_text_rejects_non_char_boundaries() {
        let text = "👍hello";
        let entity = MessageEntity {
            entity_type: "bot_command".to_string(),
            offset: 0,
            length: 2,
        };

        assert_eq!(TelegramGateway::bot_command_text(text, &entity), None);
    }

    #[test]
    fn is_bot_command_extracts_team_args_and_ignores_bot_suffix() {
        let with_args = message_with_command(Some("/team devops"), "bot_command", 0, 5);
        assert_eq!(TelegramGateway::is_bot_command(&with_args), Some("devops"));

        let in_group = message_with_command(Some("/team@mybot list"), "bot_command", 0, 12);
        assert_eq!(TelegramGateway::is_bot_command(&in_group), Some("list"));

        let no_args = message_with_command(Some("/team"), "bot_command", 0, 5);
        assert_eq!(TelegramGateway::is_bot_command(&no_args), Some(""));
    }

    #[test]
    fn is_bot_command_ignores_non_matching_commands_and_offsets() {
        let wrong_command = message_with_command(Some("/start"), "bot_command", 0, 6);
        assert_eq!(TelegramGateway::is_bot_command(&wrong_command), None);

        let non_zero_offset = message_with_command(Some("hey /team devops"), "bot_command", 4, 5);
        assert_eq!(TelegramGateway::is_bot_command(&non_zero_offset), None);

        let wrong_entity = message_with_command(Some("/team devops"), "mention", 0, 5);
        assert_eq!(TelegramGateway::is_bot_command(&wrong_entity), None);
    }

    #[test]
    fn is_bot_command_returns_none_without_text_or_entities() {
        let no_entities = TelegramMessage {
            entities: None,
            ..message_with_command(Some("hello"), "bot_command", 0, 5)
        };
        assert_eq!(TelegramGateway::is_bot_command(&no_entities), None);

        let no_text = TelegramMessage {
            text: None,
            ..message_with_command(None, "bot_command", 0, 5)
        };
        assert_eq!(TelegramGateway::is_bot_command(&no_text), None);
    }

    #[test]
    fn platform_user_uses_chat_id_as_telegram_user_id() {
        let user = TelegramGateway::platform_user(12345);

        assert_eq!(user.platform, "telegram");
        assert_eq!(user.user_id, "12345");
        assert!(user.display_name.is_none());
    }

    #[test]
    fn is_bot_command_skips_non_command_entities_before_team_command() {
        let msg = TelegramMessage {
            entities: Some(vec![
                MessageEntity {
                    entity_type: "mention".to_string(),
                    offset: 0,
                    length: 3,
                },
                MessageEntity {
                    entity_type: "bot_command".to_string(),
                    offset: 0,
                    length: 5,
                },
            ]),
            chat: Chat {
                chat_type: "group".to_string(),
                ..message_with_command(Some("/team list"), "bot_command", 0, 5).chat
            },
            ..message_with_command(Some("/team list"), "bot_command", 0, 5)
        };

        assert_eq!(TelegramGateway::is_bot_command(&msg), Some("list"));
    }

    #[test]
    fn is_bot_command_handles_empty_entity_list_and_multi_word_args() {
        let empty_entities = TelegramMessage {
            entities: Some(vec![]),
            ..message_with_command(Some("/team"), "bot_command", 0, 5)
        };
        assert_eq!(TelegramGateway::is_bot_command(&empty_entities), None);

        let multi_word = message_with_command(Some("/team list all active"), "bot_command", 0, 5);
        assert_eq!(
            TelegramGateway::is_bot_command(&multi_word),
            Some("list all active")
        );
    }

    #[test]
    fn platform_user_supports_negative_chat_ids() {
        let user = TelegramGateway::platform_user(-100123456789);

        assert_eq!(user.platform, "telegram");
        assert_eq!(user.user_id, "-100123456789");
        assert!(user.display_name.is_none());
    }
}
