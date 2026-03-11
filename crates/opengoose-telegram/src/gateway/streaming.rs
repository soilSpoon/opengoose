//! `StreamResponder` implementation for Telegram draft-based streaming.

use async_trait::async_trait;
use tracing::debug;

use opengoose_core::message_utils::truncate_for_display;
use opengoose_core::{DraftHandle, StreamResponder};

use super::types::{SentMessage, TelegramResponse};
use super::{TelegramGateway, TELEGRAM_MAX_LEN};

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
