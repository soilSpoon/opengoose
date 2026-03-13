//! Slack message sending and draft management.

use tracing::{debug, warn};

use opengoose_core::DraftHandle;
use opengoose_core::message_utils::{split_message, truncate_for_display};

use crate::types::{ChatUpdateResponse, PostMessageResponse};

use super::{SLACK_MAX_LEN, SlackGateway};

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
            anyhow::bail!("chat.update failed: {}", resp.error.unwrap_or_default());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use opengoose_core::DraftHandle;
    use opengoose_core::message_utils::{split_message, truncate_for_display};

    use super::SLACK_MAX_LEN;

    // --- SLACK_MAX_LEN constant ---

    #[test]
    fn test_slack_max_len_value() {
        assert_eq!(SLACK_MAX_LEN, 4000);
    }

    // --- split_message at Slack's limit (used by post_message) ---

    #[test]
    fn test_split_short_message_is_single_chunk() {
        let chunks = split_message("hello slack", SLACK_MAX_LEN);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello slack");
    }

    #[test]
    fn test_split_empty_message_is_single_chunk() {
        let chunks = split_message("", SLACK_MAX_LEN);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");
    }

    #[test]
    fn test_split_exactly_at_slack_limit_is_one_chunk() {
        let text = "a".repeat(SLACK_MAX_LEN);
        let chunks = split_message(&text, SLACK_MAX_LEN);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_split_one_over_slack_limit_is_two_chunks() {
        let text = "a".repeat(SLACK_MAX_LEN + 1);
        let chunks = split_message(&text, SLACK_MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), SLACK_MAX_LEN);
        assert_eq!(chunks[1].len(), 1);
    }

    #[test]
    fn test_split_large_message_content_preserved() {
        let text = "x".repeat(SLACK_MAX_LEN * 3);
        let chunks = split_message(&text, SLACK_MAX_LEN);
        let reconstructed = chunks.join("");
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn test_split_prefers_newline_boundaries() {
        // Build a message that has a newline before the limit
        let mut text = "a".repeat(3900);
        text.push('\n');
        text.push_str(&"b".repeat(200));
        let chunks = split_message(&text, SLACK_MAX_LEN);
        // Should split at the newline, giving two clean chunks
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 3900);
    }

    // --- truncate_for_display at Slack's limit (used by update_draft_message) ---

    #[test]
    fn test_truncate_short_content_unchanged() {
        let content = "partial response content";
        assert_eq!(truncate_for_display(content, SLACK_MAX_LEN), content);
    }

    #[test]
    fn test_truncate_exactly_at_limit_unchanged() {
        let content = "a".repeat(SLACK_MAX_LEN);
        assert_eq!(
            truncate_for_display(&content, SLACK_MAX_LEN).len(),
            SLACK_MAX_LEN
        );
    }

    #[test]
    fn test_truncate_over_limit_fits_within_limit() {
        let content = "a".repeat(SLACK_MAX_LEN + 500);
        let result = truncate_for_display(&content, SLACK_MAX_LEN);
        assert_eq!(result.len(), SLACK_MAX_LEN);
    }

    #[test]
    fn test_truncate_unicode_safe_at_slack_limit() {
        // Emoji spanning bytes at the boundary must not create invalid UTF-8
        let mut content = "a".repeat(SLACK_MAX_LEN - 1);
        content.push('\u{1F4AC}'); // speech bubble — 4-byte emoji
        content.push_str("overflow");
        let result = truncate_for_display(&content, SLACK_MAX_LEN);
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
        assert!(result.len() <= SLACK_MAX_LEN);
    }

    // --- DraftHandle structure (produced by create_draft_message) ---

    #[test]
    fn test_draft_handle_fields_store_channel_and_ts() {
        let handle = DraftHandle {
            message_id: "1234567890.123456".to_string(),
            channel_id: "C0123456".to_string(),
        };
        assert_eq!(handle.channel_id, "C0123456");
        assert_eq!(handle.message_id, "1234567890.123456");
    }

    #[test]
    fn test_draft_handle_clone_is_independent() {
        let handle = DraftHandle {
            message_id: "ts_original".to_string(),
            channel_id: "C_original".to_string(),
        };
        let cloned = handle.clone();
        assert_eq!(cloned.message_id, "ts_original");
        assert_eq!(cloned.channel_id, "C_original");
    }
}
