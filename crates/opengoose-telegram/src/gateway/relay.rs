use tracing::{debug, error, info};

use goose::gateway::{Gateway, OutgoingMessage};

use opengoose_core::{StreamResponder, ThrottlePolicy};

use super::{TELEGRAM_MAX_LEN, TelegramGateway, TelegramMessage, Update, User};

impl TelegramGateway {
    pub(crate) async fn handle_update(&self, update: Update, bot_username: &str) {
        let Some(message) = update.message else {
            return;
        };

        self.handle_incoming_message(message, bot_username).await;
    }

    async fn handle_incoming_message(&self, message: TelegramMessage, bot_username: &str) {
        if let Some(args) = Self::is_bot_command(&message) {
            let session_key = Self::session_key(&message.chat);
            if let Err(e) = self
                .handle_team_command(&session_key, args, message.chat.id)
                .await
            {
                error!(%e, "failed to handle /team command");
            }
            return;
        }

        let Some(text) = Self::normalized_message_text(&message, bot_username) else {
            return;
        };

        let session_key = Self::session_key(&message.chat);
        let display_name = Self::display_name(message.from.as_ref());

        if !self.bridge.is_accepting_messages() {
            info!(
                chat_id = message.chat.id,
                "ignoring telegram message during shutdown drain"
            );
            return;
        }

        debug!(
            chat_id = message.chat.id,
            chat_type = %message.chat.chat_type,
            text_len = text.len(),
            "relaying telegram message to engine"
        );

        let typing_user = Self::platform_user(message.chat.id);
        let _ = self
            .inner
            .send_message(&typing_user, OutgoingMessage::Typing)
            .await;

        let chat_id = message.chat.id.to_string();
        if let Err(e) = self
            .bridge
            .relay_and_drive_stream(
                &session_key,
                display_name,
                text,
                self as &dyn StreamResponder,
                &chat_id,
                ThrottlePolicy::telegram(),
                TELEGRAM_MAX_LEN,
            )
            .await
        {
            error!(%e, "failed to relay telegram message");
        }
    }

    fn normalized_message_text<'a>(
        message: &'a TelegramMessage,
        bot_username: &str,
    ) -> Option<&'a str> {
        let text = message.text.as_deref()?;
        let text = if message.chat.chat_type != "private" && !bot_username.is_empty() {
            Self::strip_mention(text, bot_username)
        } else {
            text
        };

        let text = text.trim();
        (!text.is_empty()).then_some(text)
    }

    pub(crate) fn display_name(user: Option<&User>) -> Option<String> {
        user.map(|user| match &user.last_name {
            Some(last_name) => format!("{} {}", user.first_name, last_name),
            None => user.first_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::gateway::types::{Chat, TelegramMessage};

    use super::*;

    fn message(chat_type: &str, text: Option<&str>) -> TelegramMessage {
        TelegramMessage {
            message_id: 1,
            chat: Chat {
                id: 1,
                chat_type: chat_type.to_string(),
            },
            from: None,
            text: text.map(str::to_string),
            entities: None,
        }
    }

    #[test]
    fn normalized_message_text_strips_group_mentions_and_whitespace() {
        let message = message("group", Some("@my_bot   hello  "));
        assert_eq!(
            TelegramGateway::normalized_message_text(&message, "my_bot"),
            Some("hello")
        );
    }

    #[test]
    fn normalized_message_text_preserves_private_message_mentions() {
        let message = message("private", Some("@my_bot hello"));
        assert_eq!(
            TelegramGateway::normalized_message_text(&message, "my_bot"),
            Some("@my_bot hello")
        );
    }

    #[test]
    fn normalized_message_text_skips_empty_results() {
        let message = message("group", Some("@my_bot   "));
        assert_eq!(
            TelegramGateway::normalized_message_text(&message, "my_bot"),
            None
        );
    }
}
