use super::mocks::{
    FailingCreateResponder, FailingFinalizeResponder, FailingUpdateResponder, MockResponder,
};
use crate::stream_orchestrator::*;

#[tokio::test]
async fn test_drive_stream_sender_dropped() {
    let (responder, calls) = MockResponder::new();
    let (tx, rx) = opengoose_types::stream_channel(16);

    tx.send(StreamChunk::Delta("partial".into())).unwrap();
    drop(tx); // Drop sender without Done

    let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("stream closed before completion")
    );
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

    let result = drive_stream(&responder, "ch", rx, ThrottlePolicy::discord(), 2000).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("stream closed before completion")
    );
    let calls = calls.lock().unwrap();
    // create_draft is called, but finalize is NOT called when buffer is empty
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], "create_draft:ch");
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
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("failed to finalize draft after stream closed unexpectedly")
    );
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
