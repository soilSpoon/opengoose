use tokio::sync::broadcast;
use tracing::{debug, debug_span, warn};

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
    let span = debug_span!(
        "drive_stream",
        channel_id = %channel_id,
        max_display_len = %max_display_len,
    )
    .entered();
    drop(span);

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
                debug!(
                    buffer_len = buffer.len(),
                    "stream completed, finalizing draft"
                );
                responder.finalize_draft(&handle, &buffer).await?;
                break;
            }
            Ok(StreamChunk::Error(e)) => {
                debug!(error = %e, buffer_len = buffer.len(), "stream error received");
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

    #[tokio::test]
    async fn test_drive_stream_error_chunk() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("partial ".into())).unwrap();
        tx.send(StreamChunk::Error("provider timeout".into()))
            .unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("provider timeout"));

        let calls = calls.lock().unwrap();
        assert_eq!(calls[0], "create_draft:ch");
        // Error path should still finalize with partial content + error message
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }

    #[tokio::test]
    async fn test_drive_stream_empty_sender_dropped() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        // Drop sender with no deltas — buffer is empty
        drop(tx);

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "");
        let calls = calls.lock().unwrap();
        // create_draft is called, but finalize is NOT called when buffer is empty
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], "create_draft:ch");
    }

    #[tokio::test]
    async fn test_drive_stream_throttled_updates() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        // Use slack throttle which requires 80 bytes delta and 1.2s interval
        tx.send(StreamChunk::Delta("a".repeat(10))).unwrap();
        tx.send(StreamChunk::Delta("b".repeat(10))).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::slack(), 2000)
            .await
            .unwrap();

        assert_eq!(result, format!("{}{}", "a".repeat(10), "b".repeat(10)));
        let calls = calls.lock().unwrap();
        // With slack throttle and small chunks, should only have create + finalize (no updates)
        assert_eq!(calls[0], "create_draft:ch");
        assert!(calls.last().unwrap().starts_with("finalize:"));
        // No update calls between create and finalize because of throttle
        assert_eq!(calls.len(), 2);
    }

    /// Mock that always returns an error from `update_draft`.
    struct FailingUpdateResponder {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl FailingUpdateResponder {
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
    impl StreamResponder for FailingUpdateResponder {
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
                message_id: "msg".into(),
                channel_id: channel_id.into(),
            })
        }
        async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("rate limited"))
        }
        async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
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
    async fn test_drive_stream_truncation() {
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        // Send content that exceeds max_display_len during streaming
        tx.send(StreamChunk::Delta("a".repeat(150))).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(
            &responder,
            "ch",
            rx,
            ThrottlePolicy::discord(), // discord allows every update
            100,                       // small max_display_len
        )
        .await
        .unwrap();

        // Full buffer is returned even though display was truncated
        assert_eq!(result.len(), 150);
        let calls = calls.lock().unwrap();
        // Update call should have truncated content
        let update_call = calls.iter().find(|c| c.starts_with("update:")).unwrap();
        let update_len: usize = update_call
            .strip_prefix("update:")
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            update_len <= 100,
            "update should be truncated to max_display_len"
        );
    }

    #[tokio::test]
    async fn test_drive_stream_discord_intermediate_updates() {
        // Discord policy has no throttle — every delta chunk must trigger an update call.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("chunk1".into())).unwrap();
        tx.send(StreamChunk::Delta("chunk2".into())).unwrap();
        tx.send(StreamChunk::Delta("chunk3".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "chunk1chunk2chunk3");
        let calls = calls.lock().unwrap();
        let update_count = calls.iter().filter(|c| c.starts_with("update:")).count();
        assert_eq!(
            update_count, 3,
            "discord policy: one update per delta chunk"
        );
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }

    #[tokio::test]
    async fn test_drive_stream_error_empty_buffer() {
        // An error chunk arriving before any deltas should still finalize and return Err.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Error("immediate error".into()))
            .unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("immediate error"));
        let calls = calls.lock().unwrap();
        assert_eq!(calls[0], "create_draft:ch");
        // Error path calls finalize_draft with the error-decorated content
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }

    #[tokio::test]
    async fn test_drive_stream_concurrent_streams() {
        // Two independent drive_stream futures run concurrently and don't interfere.
        let (r1, c1) = MockResponder::new();
        let (r2, c2) = MockResponder::new();

        let (tx1, rx1) = opengoose_types::stream_channel(16);
        let (tx2, rx2) = opengoose_types::stream_channel(16);

        tx1.send(StreamChunk::Delta("stream1".into())).unwrap();
        tx1.send(StreamChunk::Done).unwrap();
        tx2.send(StreamChunk::Delta("stream2".into())).unwrap();
        tx2.send(StreamChunk::Done).unwrap();

        let (res1, res2) = tokio::join!(
            drive_stream(&r1, "ch1", rx1, ThrottlePolicy::discord(), 2000),
            drive_stream(&r2, "ch2", rx2, ThrottlePolicy::discord(), 2000),
        );

        assert_eq!(res1.unwrap(), "stream1");
        assert_eq!(res2.unwrap(), "stream2");
        assert_eq!(c1.lock().unwrap()[0], "create_draft:ch1");
        assert_eq!(c2.lock().unwrap()[0], "create_draft:ch2");
    }

    #[tokio::test]
    async fn test_drive_stream_lagged_receiver() {
        // Overflow the broadcast buffer so the receiver gets a Lagged error.
        // drive_stream should log the lag and continue to completion.
        let (responder, calls) = MockResponder::new();

        // Capacity 4; sending 6 messages causes the first 2 to be dropped.
        let (tx, rx) = opengoose_types::stream_channel(4);

        tx.send(StreamChunk::Delta("a".into())).unwrap();
        tx.send(StreamChunk::Delta("b".into())).unwrap(); // these two get dropped
        tx.send(StreamChunk::Delta("c".into())).unwrap();
        tx.send(StreamChunk::Delta("d".into())).unwrap();
        tx.send(StreamChunk::Delta("e".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        // Stream completes successfully despite the lag
        assert!(result.is_ok(), "lagged receiver should not fail the stream");
        let calls = calls.lock().unwrap();
        assert_eq!(calls[0], "create_draft:ch");
        assert!(
            calls.last().unwrap().starts_with("finalize:"),
            "stream must finalize even after lag"
        );
    }

    #[tokio::test]
    async fn test_drive_stream_update_failure_continues() {
        // If update_draft returns an error (e.g. rate limited), drive_stream must
        // log and continue — it should still finalize successfully.
        let (responder, calls) = FailingUpdateResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("hello".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "hello");
        let calls = calls.lock().unwrap();
        assert_eq!(calls[0], "create_draft:ch");
        // finalize:5 — "hello" is 5 bytes
        assert_eq!(calls.last().unwrap(), "finalize:5");
    }

    /// Mock that fails on create_draft.
    struct FailingCreateResponder;

    #[async_trait::async_trait]
    impl StreamResponder for FailingCreateResponder {
        fn supports_streaming(&self) -> bool {
            true
        }
        fn max_message_len(&self) -> usize {
            2000
        }
        async fn create_draft(&self, _channel_id: &str) -> anyhow::Result<DraftHandle> {
            Err(anyhow::anyhow!("channel not found"))
        }
        async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn finalize_draft(
            &self,
            _handle: &DraftHandle,
            _content: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_drive_stream_create_draft_failure() {
        // If create_draft fails, the error must propagate immediately.
        let responder = FailingCreateResponder;
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("data".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("channel not found")
        );
    }

    /// Mock that fails on finalize_draft.
    struct FailingFinalizeResponder {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl FailingFinalizeResponder {
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
    impl StreamResponder for FailingFinalizeResponder {
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
                message_id: "msg".into(),
                channel_id: channel_id.into(),
            })
        }
        async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn finalize_draft(
            &self,
            _handle: &DraftHandle,
            _content: &str,
        ) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("finalize failed"))
        }
    }

    #[tokio::test]
    async fn test_drive_stream_finalize_failure_propagates() {
        // If finalize_draft fails on Done, the error must propagate.
        let (responder, _calls) = FailingFinalizeResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("content".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalize failed"));
    }

    #[tokio::test]
    async fn test_drive_stream_finalize_failure_on_sender_drop() {
        // If finalize_draft fails when sender drops (without Done), error propagates.
        let (responder, _calls) = FailingFinalizeResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("partial".into())).unwrap();
        drop(tx);

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("finalize failed"));
    }

    #[tokio::test]
    async fn test_drive_stream_error_truncated_to_max_display_len() {
        // When an error arrives with a large buffer, the finalized content
        // (buffer + error message) should be truncated before passing to finalize.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("x".repeat(90))).unwrap();
        tx.send(StreamChunk::Error("boom".into())).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 100).await;

        assert!(result.is_err());
        let calls = calls.lock().unwrap();
        // finalize is called with the truncated error message
        let finalize_call = calls.iter().find(|c| c.starts_with("finalize:")).unwrap();
        let finalized_len: usize = finalize_call
            .strip_prefix("finalize:")
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            finalized_len <= 100,
            "error message should be truncated to max_display_len, got {finalized_len}"
        );
    }

    #[tokio::test]
    async fn test_drive_stream_many_small_deltas() {
        // Many small delta chunks should all accumulate in the buffer.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(64);

        for i in 0..50 {
            tx.send(StreamChunk::Delta(format!("{i:02}"))).unwrap();
        }
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        // 50 two-digit numbers = 100 chars
        assert_eq!(result.len(), 100);
        assert!(result.starts_with("00"));
        assert!(result.ends_with("49"));
        let calls = calls.lock().unwrap();
        let update_count = calls.iter().filter(|c| c.starts_with("update:")).count();
        // Discord policy: every delta triggers an update
        assert_eq!(update_count, 50);
    }

    #[tokio::test]
    async fn test_drive_stream_telegram_throttle() {
        // Telegram throttle: 1s interval + 50 byte min delta.
        // Small fast chunks should be throttled — no updates emitted.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("short".into())).unwrap();
        tx.send(StreamChunk::Delta("msg".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::telegram(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "shortmsg");
        let calls = calls.lock().unwrap();
        // Telegram throttle should suppress updates for small/fast chunks
        let update_count = calls.iter().filter(|c| c.starts_with("update:")).count();
        assert_eq!(
            update_count, 0,
            "telegram throttle should suppress small fast updates"
        );
        assert_eq!(calls.len(), 2); // create + finalize only
    }

    #[tokio::test]
    async fn test_drive_stream_unicode_content() {
        // Ensure multi-byte UTF-8 content is handled correctly.
        let (responder, calls) = MockResponder::new();
        let (tx, rx) = opengoose_types::stream_channel(16);

        tx.send(StreamChunk::Delta("こんにちは".into())).unwrap();
        tx.send(StreamChunk::Delta("🦀".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000)
            .await
            .unwrap();

        assert_eq!(result, "こんにちは🦀");
        let calls = calls.lock().unwrap();
        assert!(calls.last().unwrap().starts_with("finalize:"));
    }
}
