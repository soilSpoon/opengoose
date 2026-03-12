use std::time::{Duration, Instant};

use super::super::protocol::ConnectionState;

/// Tracks the state of a single remote agent connection.
#[derive(Debug, Clone)]
pub struct RemoteAgent {
    /// The agent's registered name.
    pub name: String,
    /// Capabilities advertised during handshake.
    pub capabilities: Vec<String>,
    /// When the connection was established.
    pub connected_at: Instant,
    /// Last heartbeat received.
    pub last_heartbeat: Instant,
    /// Remote endpoint URL (for display/diagnostics).
    pub endpoint: String,
    /// Current lifecycle state of the connection.
    pub connection_state: ConnectionState,
}

impl RemoteAgent {
    /// Returns true if the agent has not sent a heartbeat within the timeout.
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_heartbeat.elapsed() > timeout
    }
}

/// Configuration for the remote agent registry.
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// How often heartbeats are expected (seconds).
    pub heartbeat_interval_secs: u64,
    /// How long to wait before considering a connection stale.
    pub heartbeat_timeout_secs: u64,
    /// Simple API key validation (in production, use JWT or similar).
    pub api_keys: Vec<String>,
    /// Maximum replayable outbound events retained per remote agent.
    pub replay_buffer_capacity: usize,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            api_keys: Vec::new(),
            replay_buffer_capacity: 128,
        }
    }
}
