use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};

use opengoose_types::SessionKey;

use super::TelegramGateway;

impl TelegramGateway {
    pub(crate) async fn deliver_outgoing_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        let raw_user = self.raw_recipient(user, &message).await;
        self.inner.send_message(&raw_user, message).await
    }

    async fn raw_recipient(&self, user: &PlatformUser, message: &OutgoingMessage) -> PlatformUser {
        PlatformUser {
            platform: user.platform.clone(),
            user_id: self.raw_channel_id(&user.user_id, message).await,
            display_name: user.display_name.clone(),
        }
    }

    async fn raw_channel_id(&self, user_id: &str, message: &OutgoingMessage) -> String {
        match message {
            OutgoingMessage::Text { body } => {
                self.bridge
                    .route_outgoing_text(user_id, body, "telegram")
                    .await
            }
            _ => SessionKey::from_stable_id(user_id).channel_id,
        }
    }
}
