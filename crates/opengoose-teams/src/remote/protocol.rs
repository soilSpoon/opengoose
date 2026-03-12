/// Protocol message types and connection state models for the remote agent gateway.
use serde::{Deserialize, Serialize};

/// Protocol message types exchanged over the WebSocket connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    /// Client → Server: initial authentication.
    Handshake {
        agent_name: String,
        api_key: String,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    /// Server → Client: handshake result.
    HandshakeAck {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Bidirectional: keep-alive ping.
    Heartbeat {
        #[serde(default = "default_timestamp")]
        timestamp: u64,
    },
    /// Server → Client or Client → Server: relay a message.
    MessageRelay {
        from: String,
        to: String,
        payload: String,
    },
    /// Server → Client: broadcast from a channel.
    Broadcast {
        from: String,
        channel: String,
        payload: String,
    },
    /// Client → Server: agent wants to disconnect gracefully.
    Disconnect { reason: String },
    /// Server → Client: error notification.
    Error { message: String },
    /// Client → Server: reconnect after a drop, providing the last seen event ID.
    Reconnect {
        #[serde(default)]
        last_event_id: u64,
    },
    /// Server → Client: reconnect acknowledgement.
    ReconnectAck {
        success: bool,
        /// Number of buffered outbound events replayed since `last_event_id`.
        replayed_events: u64,
    },
}

/// Lifecycle state of a remote agent connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Initial handshake in progress.
    Connecting,
    /// Fully authenticated and operational.
    Connected,
    /// Graceful teardown in progress.
    Disconnecting,
    /// Re-connecting after a drop.
    Reconnecting,
}

/// Aggregate metrics for the remote agent gateway.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionMetrics {
    /// Number of agents currently connected.
    pub active_connections: usize,
    /// Total agents that have connected since startup.
    pub total_connects: u64,
    /// Total agents that have disconnected since startup.
    pub total_disconnects: u64,
    /// Average connection uptime in seconds across all sessions.
    pub avg_uptime_secs: u64,
}

pub(crate) fn default_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
