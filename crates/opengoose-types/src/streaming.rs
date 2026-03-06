use tokio::sync::broadcast;

/// Individual chunk of a streaming response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Incremental text fragment from the LLM.
    Delta(String),
    /// Stream completed successfully.
    Done,
    /// An error occurred during generation.
    Error(String),
}

/// Unique identifier for a streaming session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(pub String);

impl StreamId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Create a new broadcast channel for streaming chunks.
///
/// Returns `(sender, receiver)`. The sender is used by the LLM/engine side,
/// and the receiver is consumed by the gateway's `drive_stream` loop.
pub fn stream_channel(
    capacity: usize,
) -> (
    broadcast::Sender<StreamChunk>,
    broadcast::Receiver<StreamChunk>,
) {
    let (tx, rx) = broadcast::channel(capacity);
    (tx, rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_stream_id_new_and_display() {
        let id = StreamId::new("stream-42");
        assert_eq!(id.0, "stream-42");
        assert_eq!(format!("{id}"), "stream-42");
    }

    #[test]
    fn test_stream_id_equality_and_hash() {
        let a = StreamId::new("abc");
        let b = StreamId::new("abc");
        let c = StreamId::new("def");
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a.clone());
        set.insert(b);
        assert_eq!(set.len(), 1);
        set.insert(c);
        assert_eq!(set.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_channel_send_recv() {
        let (tx, mut rx) = stream_channel(8);
        tx.send(StreamChunk::Delta("hello ".into())).unwrap();
        tx.send(StreamChunk::Delta("world".into())).unwrap();
        tx.send(StreamChunk::Done).unwrap();

        let chunk1 = rx.recv().await.unwrap();
        assert!(matches!(chunk1, StreamChunk::Delta(s) if s == "hello "));
        let chunk2 = rx.recv().await.unwrap();
        assert!(matches!(chunk2, StreamChunk::Delta(s) if s == "world"));
        let chunk3 = rx.recv().await.unwrap();
        assert!(matches!(chunk3, StreamChunk::Done));
    }

    #[tokio::test]
    async fn test_stream_channel_error_variant() {
        let (tx, mut rx) = stream_channel(4);
        tx.send(StreamChunk::Error("timeout".into())).unwrap();

        let chunk = rx.recv().await.unwrap();
        assert!(matches!(chunk, StreamChunk::Error(s) if s == "timeout"));
    }
}
