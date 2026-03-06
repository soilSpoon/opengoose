use async_trait::async_trait;

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

    /// Send an initial placeholder message and return an editable handle.
    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle>;

    /// Update the draft message with new content (partial response).
    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()>;

    /// Finalize the draft with the complete response.
    ///
    /// If the final content exceeds the platform's message size limit,
    /// implementations should edit the original message with the first
    /// chunk and send the remainder as new messages.
    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()>;
}
