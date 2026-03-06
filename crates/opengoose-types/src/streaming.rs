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
