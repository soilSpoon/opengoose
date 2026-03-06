use tokio::sync::broadcast;
use tracing::warn;

use opengoose_types::StreamChunk;

use crate::message_utils::truncate_for_display;
use crate::stream_responder::StreamResponder;
use crate::throttle::ThrottlePolicy;

/// Drive a streaming response: consume token chunks from the broadcast
/// receiver, throttle updates, and edit the draft message on the platform.
///
/// Returns the full accumulated text on success.
///
/// # Arguments
/// * `responder` — platform-specific implementation that creates/edits messages
/// * `channel_id` — target channel for the draft message
/// * `rx` — receiver of [`StreamChunk`] events from the LLM/engine
/// * `throttle` — platform-appropriate rate limiting policy
/// * `max_display_len` — platform message size limit for intermediate updates
pub async fn drive_stream(
    responder: &dyn StreamResponder,
    channel_id: &str,
    mut rx: broadcast::Receiver<StreamChunk>,
    mut throttle: ThrottlePolicy,
    max_display_len: usize,
) -> anyhow::Result<String> {
    let handle = responder.create_draft(channel_id).await?;
    let mut buffer = String::new();

    loop {
        match rx.recv().await {
            Ok(StreamChunk::Delta(delta)) => {
                buffer.push_str(&delta);
                if throttle.should_update(buffer.len()) {
                    let display = truncate_for_display(&buffer, max_display_len);
                    if let Err(e) = responder.update_draft(&handle, display).await {
                        warn!(%e, "failed to update draft, continuing to buffer");
                    }
                    throttle.record_update(buffer.len());
                }
            }
            Ok(StreamChunk::Done) => {
                responder.finalize_draft(&handle, &buffer).await?;
                break;
            }
            Ok(StreamChunk::Error(e)) => {
                let error_msg = format!("{buffer}\n\n--- Error: {e}");
                let display = truncate_for_display(&error_msg, max_display_len);
                let _ = responder.finalize_draft(&handle, display).await;
                return Err(anyhow::anyhow!(e));
            }
            Err(broadcast::error::RecvError::Closed) => {
                // Sender dropped without sending Done — finalize with what we have
                if !buffer.is_empty() {
                    responder.finalize_draft(&handle, &buffer).await?;
                }
                break;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(skipped = n, "stream receiver lagged, some tokens lost");
                continue;
            }
        }
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_responder::DraftHandle;
    use std::sync::{Arc, Mutex};

    /// Test implementation of StreamResponder that records calls.
    struct MockResponder {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl MockResponder {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    calls: calls.clone(),
                },
                calls,
            )
        }
    }

    #[async_trait::async_trait]
    impl StreamResponder for MockResponder {
        fn supports_streaming(&self) -> bool {
            true
        }

        fn max_message_len(&self) -> usize {
            2000
        }

        async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("create_draft:{channel_id}"));
            Ok(DraftHandle {
                message_id: "mock-msg-1".into(),
                channel_id: channel_id.into(),
            })
        }

        async fn update_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("update:{}", content.len()));
            Ok(())
        }

        async fn send_new_message(&self, _channel_id: &str, content: &str) -> anyhow::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("send_new:{}", content.len()));
            Ok(())
        }

        async fn finalize_draft(&self, _handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("finalize:{}", content.len()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_drive_stream_basic() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        // Send chunks then done
        tx.send(StreamChunk::Delta("Hello ".into())).unwrap();
        tx.send(StreamChunk::Delta("world!".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(
            &responder,
            "test-channel",
            rx,
            ThrottlePolicy::discord(),
            2000,
        )
        .await
        .unwrap();

        assert_eq!(result, "Hello world!");
        let calls = calls.lock().unwrap();
        assert_eq!(calls[0], "create_draft:test-channel");
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }

    #[tokio::test]
    async fn test_drive_stream_sender_dropped() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("partial".into())).unwrap();
        drop(tx); // Drop sender without Done

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "partial");
        let calls = calls.lock().unwrap();
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }
}
