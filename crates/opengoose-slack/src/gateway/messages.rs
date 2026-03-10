//! Slack message sending and draft management.

use tracing::{debug, warn};

use opengoose_core::message_utils::{split_message, truncate_for_display};
use opengoose_core::DraftHandle;

use crate::types::{ChatUpdateResponse, PostMessageResponse};

use super::{SlackGateway, SLACK_MAX_LEN};

impl SlackGateway {
    /// Send a message to a Slack channel via Web API.
    pub(super) async fn post_message(&self, channel: &str, text: &str) -> anyhow::Result<()> {
        debug!(channel = %channel, text_len = text.len(), "posting slack message");
        for chunk in split_message(text, SLACK_MAX_LEN) {
            let resp: PostMessageResponse = self
                .client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(&self.bot_token)
                .json(&serde_json::json!({
                    "channel": channel,
                    "text": chunk,
                }))
                .send()
                .await?
                .json()
                .await?;

            if !resp.ok {
                warn!(
                    "chat.postMessage failed: {}",
                    resp.error.unwrap_or_default()
                );
            }
        }
        Ok(())
    }

    /// Respond ephemerally to a slash command via response_url.
    pub(super) async fn respond_ephemeral(&self, response_url: &str, text: &str) {
        let _ = self
            .client
            .post(response_url)
            .json(&serde_json::json!({
                "response_type": "ephemeral",
                "text": text,
            }))
            .send()
            .await;
    }

    /// Create a draft placeholder message in a Slack channel.
    pub(super) async fn create_draft_message(&self, channel: &str) -> anyhow::Result<DraftHandle> {
        debug!(channel = %channel, "creating slack draft");
        let resp: PostMessageResponse = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({
                "channel": channel,
                "text": "Thinking...",
            }))
            .send()
            .await?
            .json()
            .await?;

        let ts = resp
            .ts
            .ok_or_else(|| anyhow::anyhow!("chat.postMessage returned no ts"))?;
        debug!(channel = %channel, ts = %ts, "slack draft created");
        Ok(DraftHandle {
            message_id: ts,
            channel_id: channel.to_string(),
        })
    }

    /// Update an existing draft message with new content.
    pub(super) async fn update_draft_message(
        &self,
        handle: &DraftHandle,
        content: &str,
    ) -> anyhow::Result<()> {
        debug!(
            channel = %handle.channel_id,
            ts = %handle.message_id,
            content_len = content.len(),
            "updating slack draft"
        );
        let display = truncate_for_display(content, SLACK_MAX_LEN);
        let resp: ChatUpdateResponse = self
            .client
            .post("https://slack.com/api/chat.update")
            .bearer_auth(&self.bot_token)
            .json(&serde_json::json!({
                "channel": handle.channel_id,
                "ts": handle.message_id,
                "text": display,
            }))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("chat.update failed");
        }
        Ok(())
    }
}
