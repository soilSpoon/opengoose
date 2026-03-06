use async_trait::async_trait;

use crate::message_utils::split_message;

/// Handle to an editable "draft" message on a platform.
///
/// Created by [`StreamResponder::create_draft`] and used to update
/// the message content as LLM tokens arrive.
#[derive(Debug, Clone)]
pub struct DraftHandle {
    /// Platform-specific message identifier.
    /// - Discord: message ID (u64 as string)
    /// - Slack: message timestamp (`ts` field)
    /// - Telegram: message_id (i64 as string)
    pub message_id: String,
    /// Channel/conversation where the draft lives.
    pub channel_id: String,
}

/// Capability trait for gateways that support streaming responses
/// via message editing.
///
/// This is intentionally separate from the `goose::gateway::Gateway` trait
/// to avoid modifying the external dependency. Gateways that support
/// streaming implement both `Gateway` and `StreamResponder`.
#[async_trait]
pub trait StreamResponder: Send + Sync {
    /// Whether this gateway supports edit-based streaming.
    fn supports_streaming(&self) -> bool;

    /// Maximum message length for this platform (used by the default `finalize_draft`).
    fn max_message_len(&self) -> usize;

    /// Send an initial placeholder message and return an editable handle.
    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle>;

    /// Update the draft message with new content (partial response).
    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()>;

    /// Send a new (non-edit) message to the channel. Used by the default
    /// `finalize_draft` to deliver overflow chunks.
    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()>;

    /// Finalize the draft with the complete response.
    ///
    /// The default implementation splits `content` using [`split_message`],
    /// edits the original message with the first chunk, and sends the
    /// remainder as new messages via [`send_new_message`].
    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        let chunks = split_message(content, self.max_message_len());

        // Edit the original message with the first chunk
        self.update_draft(handle, chunks[0]).await?;

        // Send remaining chunks as new messages
        for chunk in &chunks[1..] {
            if let Err(e) = self.send_new_message(&handle.channel_id, chunk).await {
                tracing::error!(%e, "failed to send overflow chunk");
            }
        }
        Ok(())
    }
}
