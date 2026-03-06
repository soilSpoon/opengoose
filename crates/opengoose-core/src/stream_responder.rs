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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Mock StreamResponder that records all calls.
    struct MockResponder {
        max_len: usize,
        updated: Arc<Mutex<Vec<String>>>,
        sent: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl MockResponder {
        fn new(max_len: usize) -> Self {
            Self {
                max_len,
                updated: Arc::new(Mutex::new(Vec::new())),
                sent: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl StreamResponder for MockResponder {
        fn supports_streaming(&self) -> bool {
            true
        }

        fn max_message_len(&self) -> usize {
            self.max_len
        }

        async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
            Ok(DraftHandle {
                message_id: "draft-1".into(),
                channel_id: channel_id.into(),
            })
        }

        async fn update_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
            self.updated.lock().unwrap().push(content.to_string());
            Ok(())
        }

        async fn send_new_message(
            &self,
            channel_id: &str,
            content: &str,
        ) -> anyhow::Result<()> {
            self.sent
                .lock()
                .unwrap()
                .push((channel_id.to_string(), content.to_string()));
            Ok(())
        }
    }

    #[test]
    fn test_draft_handle_debug_clone() {
        let handle = DraftHandle {
            message_id: "123".into(),
            channel_id: "ch1".into(),
        };
        let cloned = handle.clone();
        assert_eq!(cloned.message_id, "123");
        assert_eq!(cloned.channel_id, "ch1");
        assert!(format!("{:?}", handle).contains("123"));
    }

    #[tokio::test]
    async fn test_finalize_draft_short_message() {
        let responder = MockResponder::new(2000);
        let handle = responder.create_draft("ch1").await.unwrap();
        responder.finalize_draft(&handle, "hello").await.unwrap();

        let updated = responder.updated.lock().unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0], "hello");

        let sent = responder.sent.lock().unwrap();
        assert!(sent.is_empty());
    }

    #[tokio::test]
    async fn test_finalize_draft_splits_long_message() {
        let responder = MockResponder::new(10);
        let handle = responder.create_draft("ch1").await.unwrap();
        let content = "a".repeat(25);
        responder.finalize_draft(&handle, &content).await.unwrap();

        let updated = responder.updated.lock().unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].len(), 10);

        let sent = responder.sent.lock().unwrap();
        assert!(!sent.is_empty());
        for (channel_id, _) in sent.iter() {
            assert_eq!(channel_id, "ch1");
        }
    }

    #[tokio::test]
    async fn test_supports_streaming() {
        let responder = MockResponder::new(100);
        assert!(responder.supports_streaming());
        assert_eq!(responder.max_message_len(), 100);
    }
}
