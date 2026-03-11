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

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Update { message_id: String, content: String },
        SendAttempt { channel_id: String, content: String },
    }

    struct RecordingResponder {
        max_message_len: usize,
        calls: Arc<Mutex<Vec<Call>>>,
        fail_on_chunk: Option<String>,
    }

    impl RecordingResponder {
        fn new(max_message_len: usize) -> Self {
            Self {
                max_message_len,
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_on_chunk: None,
            }
        }

        fn with_failed_chunk(max_message_len: usize, chunk: &str) -> Self {
            Self {
                max_message_len,
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_on_chunk: Some(chunk.to_string()),
            }
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl StreamResponder for RecordingResponder {
        fn supports_streaming(&self) -> bool {
            true
        }

        fn max_message_len(&self) -> usize {
            self.max_message_len
        }

        async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
            Ok(DraftHandle {
                message_id: "draft-1".into(),
                channel_id: channel_id.into(),
            })
        }

        async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push(Call::Update {
                message_id: handle.message_id.clone(),
                content: content.to_string(),
            });
            Ok(())
        }

        async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push(Call::SendAttempt {
                channel_id: channel_id.to_string(),
                content: content.to_string(),
            });

            if self.fail_on_chunk.as_deref() == Some(content) {
                return Err(anyhow::anyhow!("send failed for {content}"));
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn finalize_draft_updates_single_chunk_without_overflow() {
        let responder = RecordingResponder::new(50);
        let handle = responder.create_draft("channel-1").await.unwrap();

        responder
            .finalize_draft(&handle, "short response")
            .await
            .unwrap();

        assert_eq!(
            responder.calls(),
            vec![Call::Update {
                message_id: "draft-1".into(),
                content: "short response".into(),
            }]
        );
    }

    #[tokio::test]
    async fn finalize_draft_splits_message_and_sends_overflow_chunks() {
        let responder = RecordingResponder::new(10);
        let handle = responder.create_draft("channel-1").await.unwrap();

        responder
            .finalize_draft(&handle, "aaaaa\nbbbbb\nccccc")
            .await
            .unwrap();

        assert_eq!(
            responder.calls(),
            vec![
                Call::Update {
                    message_id: "draft-1".into(),
                    content: "aaaaa".into(),
                },
                Call::SendAttempt {
                    channel_id: "channel-1".into(),
                    content: "bbbbb".into(),
                },
                Call::SendAttempt {
                    channel_id: "channel-1".into(),
                    content: "ccccc".into(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn finalize_draft_continues_after_overflow_send_failure() {
        let responder = RecordingResponder::with_failed_chunk(10, "bbbbb");
        let handle = responder.create_draft("channel-1").await.unwrap();

        responder
            .finalize_draft(&handle, "aaaaa\nbbbbb\nccccc")
            .await
            .unwrap();

        assert_eq!(
            responder.calls(),
            vec![
                Call::Update {
                    message_id: "draft-1".into(),
                    content: "aaaaa".into(),
                },
                Call::SendAttempt {
                    channel_id: "channel-1".into(),
                    content: "bbbbb".into(),
                },
                Call::SendAttempt {
                    channel_id: "channel-1".into(),
                    content: "ccccc".into(),
                },
            ]
        );
    }
}
