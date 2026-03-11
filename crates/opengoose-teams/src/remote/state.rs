use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

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
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            api_keys: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ConnectionState, RemoteAgent};

    #[test]
    fn remote_agent_staleness() {
        let agent = RemoteAgent {
            name: "test".into(),
            capabilities: vec![],
            connected_at: Instant::now(),
            last_heartbeat: Instant::now() - Duration::from_secs(100),
            endpoint: "ws://test".into(),
            connection_state: ConnectionState::Connected,
        };
        assert!(agent.is_stale(Duration::from_secs(90)));
        assert!(!agent.is_stale(Duration::from_secs(200)));
    }

    #[test]
    fn connection_state_serialization() {
        for (state, expected) in [
            (ConnectionState::Connecting, "connecting"),
            (ConnectionState::Connected, "connected"),
            (ConnectionState::Disconnecting, "disconnecting"),
            (ConnectionState::Reconnecting, "reconnecting"),
        ] {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
        }
    }
}
