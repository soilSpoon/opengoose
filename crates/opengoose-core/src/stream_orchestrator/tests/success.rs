use super::mocks::MockResponder;
use crate::stream_orchestrator::*;

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
