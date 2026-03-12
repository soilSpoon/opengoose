//! Discord outgoing-message delivery.

use tracing::{debug, error, warn};

use twilight_model::id::Id;
use twilight_model::id::marker::ChannelMarker;

use goose::gateway::{OutgoingMessage, PlatformUser};
use opengoose_core::StreamResponder;

use super::DiscordGateway;
use super::helpers::split_discord_chunks;

impl DiscordGateway {
    pub(super) async fn send_outgoing_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        match message {
            OutgoingMessage::Typing => self.ensure_active_draft(&user.user_id).await,
            OutgoingMessage::Text { body } => self.send_text_message(user, &body).await,
        }

        Ok(())
    }

    async fn send_text_message(&self, user: &PlatformUser, body: &str) {
        debug!(
            user_id = %user.user_id,
            body_len = body.len(),
            "discord outgoing text message"
        );

        let channel_id = self
            .bridge
            .route_outgoing_text(&user.user_id, body, "discord")
            .await;

        match self.take_active_draft(&user.user_id) {
            Some(handle) => {
                if let Err(error) = self.finalize_draft(&handle, body).await {
                    warn!(error = %error, "failed to finalize draft; falling back to new message");
                    self.send_to_routed_channel(&channel_id, body, false).await;
                }
            }
            None => self.send_to_routed_channel(&channel_id, body, true).await,
        }
    }

    pub(super) async fn send_to_channel(&self, channel_id: Id<ChannelMarker>, body: &str) {
        let chunks = split_discord_chunks(body);
        debug!(
            channel_id = %channel_id,
            chunks = chunks.len(),
            body_len = body.len(),
            "sending discord message"
        );

        for chunk in chunks {
            if let Err(error) = self.http.create_message(channel_id).content(chunk).await {
                error!(%error, channel_id = %channel_id, "failed to send discord message");
            }
        }
    }

    async fn send_to_routed_channel(&self, channel_id: &str, body: &str, warn_on_invalid: bool) {
        let parsed_channel_id = match channel_id.parse::<u64>() {
            Ok(id) => Id::<ChannelMarker>::new(id),
            Err(_) => {
                if warn_on_invalid {
                    warn!(channel_id = %channel_id, "invalid channel id");
                }
                return;
            }
        };

        self.send_to_channel(parsed_channel_id, body).await;
    }
}

#[cfg(test)]
mod tests {
    use twilight_model::id::Id;
    use twilight_model::id::marker::ChannelMarker;

    #[test]
    fn valid_channel_id_parses_as_snowflake() {
        let parsed = "1234567890123456"
            .parse::<u64>()
            .map(Id::<ChannelMarker>::new)
            .expect("channel id should parse");

        assert_eq!(parsed, Id::<ChannelMarker>::new(1234567890123456));
    }

    #[test]
    fn invalid_channel_ids_fail_to_parse() {
        assert!("not-a-snowflake".parse::<u64>().is_err());
        assert!("-12345".parse::<u64>().is_err());
        assert!("".parse::<u64>().is_err());
        assert!("1234.56".parse::<u64>().is_err());
        assert!(" 123456".parse::<u64>().is_err());
    }
}
