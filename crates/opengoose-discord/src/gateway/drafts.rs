//! Discord draft placeholder and streaming helpers.

use tracing::debug;

use twilight_model::id::Id;
use twilight_model::id::marker::{ChannelMarker, MessageMarker};

use opengoose_core::DraftHandle;
use opengoose_core::message_utils::truncate_for_display;
use opengoose_types::SessionKey;

use super::{DISCORD_MAX_LEN, DiscordGateway};

impl DiscordGateway {
    pub(super) async fn ensure_active_draft(&self, user_id: &str) {
        debug!(user_id = %user_id, "discord outgoing typing indicator");

        if self.has_active_draft(user_id) {
            return;
        }

        let session_key = SessionKey::from_stable_id(user_id);
        let channel_id = session_key.channel_id;

        match self.create_draft_message(&channel_id).await {
            Ok(handle) => self.store_active_draft(user_id, handle),
            Err(error) => debug!(error = %error, "failed to create typing draft"),
        }
    }

    pub(super) fn take_active_draft(&self, user_id: &str) -> Option<DraftHandle> {
        self.active_drafts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(user_id)
    }

    pub(super) async fn create_draft_message(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<DraftHandle> {
        debug!(channel_id = %channel_id, "creating discord draft");
        let channel_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        let message = self
            .http
            .create_message(channel_id)
            .content("Thinking...")
            .await?
            .model()
            .await?;
        debug!(
            channel_id = %channel_id,
            message_id = %message.id,
            "discord draft created"
        );

        Ok(DraftHandle {
            message_id: message.id.to_string(),
            channel_id: channel_id.to_string(),
        })
    }

    pub(super) async fn update_draft_message(
        &self,
        handle: &DraftHandle,
        content: &str,
    ) -> anyhow::Result<()> {
        debug!(
            channel_id = %handle.channel_id,
            message_id = %handle.message_id,
            content_len = content.len(),
            "updating discord draft"
        );

        let channel_id = Id::<ChannelMarker>::new(handle.channel_id.parse()?);
        let message_id = Id::<MessageMarker>::new(handle.message_id.parse()?);
        let display = truncate_for_display(content, DISCORD_MAX_LEN);

        self.http
            .update_message(channel_id, message_id)
            .content(Some(display))
            .await?;

        Ok(())
    }

    pub(super) async fn send_draft_overflow(
        &self,
        channel_id: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let channel_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        self.http
            .create_message(channel_id)
            .content(content)
            .await?;
        Ok(())
    }

    fn has_active_draft(&self, user_id: &str) -> bool {
        self.active_drafts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(user_id)
    }

    fn store_active_draft(&self, user_id: &str, handle: DraftHandle) {
        self.active_drafts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(user_id.to_string(), handle);
    }
}

#[cfg(test)]
mod tests {
    use opengoose_core::message_utils::truncate_for_display;

    use crate::gateway::DISCORD_MAX_LEN;

    #[test]
    fn update_draft_truncates_content_to_discord_limit() {
        let content = "a".repeat(DISCORD_MAX_LEN + 25);
        let display = truncate_for_display(&content, DISCORD_MAX_LEN);

        assert_eq!(display.len(), DISCORD_MAX_LEN);
    }

    #[test]
    fn update_draft_truncation_preserves_valid_utf8() {
        let mut content = "a".repeat(DISCORD_MAX_LEN - 1);
        content.push('\u{1F4AC}');
        content.push_str("overflow");

        let display = truncate_for_display(&content, DISCORD_MAX_LEN);
        assert!(std::str::from_utf8(display.as_bytes()).is_ok());
        assert!(display.len() <= DISCORD_MAX_LEN);
    }
}
