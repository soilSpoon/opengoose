//! `StreamResponder` implementation for Telegram draft-based streaming.

use async_trait::async_trait;
use tracing::debug;

use opengoose_core::message_utils::truncate_for_display;
use opengoose_core::{DraftHandle, StreamResponder};

use super::types::{SentMessage, TelegramResponse};
use super::{TELEGRAM_MAX_LEN, TelegramGateway};

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

#[cfg(test)]
mod tests {
    use opengoose_core::StreamResponder;
    use opengoose_types::EventBus;

    use super::*;
    use crate::gateway::test_support::{MockResponse, MockTelegramApi, test_gateway};

    #[tokio::test]
    async fn create_draft_posts_placeholder_and_returns_handle() {
        let api = MockTelegramApi::spawn(vec![MockResponse::json(serde_json::json!({
            "ok": true,
            "result": { "message_id": 42 }
        }))])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));

        let handle = gateway.create_draft("123").await.unwrap();

        assert_eq!(handle.message_id, "42");
        assert_eq!(handle.channel_id, "123");

        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bottest-token/sendMessage");
        assert_eq!(requests[0].body["chat_id"], "123");
        assert_eq!(requests[0].body["text"], "Thinking...");
    }

    #[tokio::test]
    async fn update_draft_truncates_content_before_editing() {
        let api = MockTelegramApi::spawn(vec![MockResponse::json(serde_json::json!({
            "ok": true,
            "result": {}
        }))])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));
        let handle = DraftHandle {
            message_id: "42".to_string(),
            channel_id: "123".to_string(),
        };
        let content = format!("{}🙂tail", "a".repeat(TELEGRAM_MAX_LEN - 1));

        gateway.update_draft(&handle, &content).await.unwrap();

        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bottest-token/editMessageText");
        assert_eq!(requests[0].body["chat_id"], "123");
        assert_eq!(requests[0].body["message_id"], 42);
        assert_eq!(
            requests[0].body["text"].as_str().unwrap(),
            "a".repeat(TELEGRAM_MAX_LEN - 1)
        );
    }

    #[tokio::test]
    async fn finalize_draft_updates_first_chunk_and_sends_overflow_message() {
        let api = MockTelegramApi::spawn(vec![
            MockResponse::json(serde_json::json!({ "ok": true, "result": {} })),
            MockResponse::json(serde_json::json!({})),
        ])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));
        let handle = DraftHandle {
            message_id: "42".to_string(),
            channel_id: "123".to_string(),
        };
        let content = format!("{}\n{}", "a".repeat(TELEGRAM_MAX_LEN - 1), "overflow");

        gateway.finalize_draft(&handle, &content).await.unwrap();

        let requests = api.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, "/bottest-token/editMessageText");
        assert_eq!(
            requests[0].body["text"].as_str().unwrap(),
            "a".repeat(TELEGRAM_MAX_LEN - 1)
        );
        assert_eq!(requests[1].path, "/bottest-token/sendMessage");
        assert_eq!(requests[1].body["chat_id"], "123");
        assert_eq!(requests[1].body["text"], "overflow");
    }
}
