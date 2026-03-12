/// Per-agent transport state and replay buffering for reconnect recovery.
use std::collections::VecDeque;

use super::protocol::ProtocolMessage;

#[derive(Debug, Clone)]
pub(super) struct ReplayEvent {
    pub(super) event_id: u64,
    pub(super) message: ProtocolMessage,
}

#[derive(Debug, Clone)]
pub(super) struct AgentTransport {
    pub(super) tx: Option<tokio::sync::mpsc::UnboundedSender<ProtocolMessage>>,
    pub(super) next_event_id: u64,
    pub(super) replay_buffer: VecDeque<ReplayEvent>,
}

impl AgentTransport {
    pub(super) fn new(tx: tokio::sync::mpsc::UnboundedSender<ProtocolMessage>) -> Self {
        Self {
            tx: Some(tx),
            next_event_id: 1,
            replay_buffer: VecDeque::new(),
        }
    }

    pub(super) fn attach(&mut self, tx: tokio::sync::mpsc::UnboundedSender<ProtocolMessage>) {
        self.tx = Some(tx);
    }

    pub(super) fn detach(&mut self) {
        self.tx = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayResult {
    Replayed(u64),
    BufferMiss,
    Unavailable,
}

pub(super) fn should_buffer_for_replay(message: &ProtocolMessage) -> bool {
    matches!(
        message,
        ProtocolMessage::MessageRelay { .. }
            | ProtocolMessage::Broadcast { .. }
            | ProtocolMessage::Disconnect { .. }
    )
}
