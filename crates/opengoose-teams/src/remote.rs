/// Remote Agent Protocol for OpenGoose.
///
/// Enables agents running on remote machines to participate in OpenGoose
/// teams over a WebSocket connection. The protocol supports:
///
/// - **Handshake**: authenticate and register the remote agent
/// - **Heartbeat**: periodic keep-alive to detect disconnections
/// - **Message relay**: forward messages between local and remote agents
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

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
}

fn default_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

/// Central registry for all connected remote agents.
///
/// Thread-safe and clonable — share across handler tasks.
#[derive(Clone)]
pub struct RemoteAgentRegistry {
    agents: Arc<RwLock<HashMap<String, RemoteAgent>>>,
    config: Arc<RemoteConfig>,
    /// Channel for sending messages to remote agents.
    /// Key: agent name, Value: sender half of an unbounded channel.
    outbound: Arc<Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<ProtocolMessage>>>>,
}

impl RemoteAgentRegistry {
    /// Create a new registry with the given configuration.
    pub fn new(config: RemoteConfig) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            outbound: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Validate an API key against the configured keys.
    ///
    /// If no keys are configured, all connections are accepted (development mode).
    pub fn validate_key(&self, key: &str) -> bool {
        self.config.api_keys.is_empty() || self.config.api_keys.iter().any(|k| k == key)
    }

    /// Register a remote agent after successful handshake.
    pub async fn register(
        &self,
        name: String,
        capabilities: Vec<String>,
        endpoint: String,
        tx: tokio::sync::mpsc::UnboundedSender<ProtocolMessage>,
    ) -> Result<(), String> {
        let agents = self.agents.read().await;
        if agents.contains_key(&name) {
            return Err(format!("agent '{}' is already connected", name));
        }
        drop(agents);

        let now = Instant::now();
        let agent = RemoteAgent {
            name: name.clone(),
            capabilities,
            connected_at: now,
            last_heartbeat: now,
            endpoint,
        };

        self.agents.write().await.insert(name.clone(), agent);
        self.outbound.lock().await.insert(name, tx);
        Ok(())
    }

    /// Remove a remote agent from the registry.
    pub async fn unregister(&self, name: &str) {
        self.agents.write().await.remove(name);
        self.outbound.lock().await.remove(name);
    }

    /// Update the heartbeat timestamp for an agent.
    pub async fn touch_heartbeat(&self, name: &str) {
        if let Some(agent) = self.agents.write().await.get_mut(name) {
            agent.last_heartbeat = Instant::now();
        }
    }

    /// Send a protocol message to a specific remote agent.
    ///
    /// Returns `true` if the message was sent, `false` if the agent is not connected.
    pub async fn send_to(&self, name: &str, msg: ProtocolMessage) -> bool {
        let outbound = self.outbound.lock().await;
        if let Some(tx) = outbound.get(name) {
            tx.send(msg).is_ok()
        } else {
            false
        }
    }

    /// List all currently connected remote agents.
    pub async fn list(&self) -> Vec<RemoteAgent> {
        self.agents.read().await.values().cloned().collect()
    }

    /// Check if a specific agent is connected.
    pub async fn is_connected(&self, name: &str) -> bool {
        self.agents.read().await.contains_key(name)
    }

    /// Get the heartbeat timeout duration from config.
    pub fn heartbeat_timeout(&self) -> Duration {
        Duration::from_secs(self.config.heartbeat_timeout_secs)
    }

    /// Get the heartbeat interval duration from config.
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.config.heartbeat_interval_secs)
    }

    /// Remove stale agents that have not sent a heartbeat within the timeout.
    ///
    /// Returns the names of agents that were removed.
    pub async fn reap_stale(&self) -> Vec<String> {
        let timeout = self.heartbeat_timeout();
        let mut agents = self.agents.write().await;
        let stale: Vec<String> = agents
            .iter()
            .filter(|(_, a)| a.is_stale(timeout))
            .map(|(name, _)| name.clone())
            .collect();

        for name in &stale {
            agents.remove(name);
        }
        drop(agents);

        let mut outbound = self.outbound.lock().await;
        for name in &stale {
            outbound.remove(name);
        }

        stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RemoteConfig {
        RemoteConfig {
            heartbeat_interval_secs: 5,
            heartbeat_timeout_secs: 15,
            api_keys: vec!["test-key-123".to_string()],
        }
    }

    #[test]
    fn protocol_message_serialization() {
        let msg = ProtocolMessage::Handshake {
            agent_name: "remote-1".into(),
            api_key: "key".into(),
            capabilities: vec!["code-review".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"handshake\""));
        assert!(json.contains("remote-1"));

        let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProtocolMessage::Handshake {
                agent_name,
                api_key,
                capabilities,
            } => {
                assert_eq!(agent_name, "remote-1");
                assert_eq!(api_key, "key");
                assert_eq!(capabilities, vec!["code-review"]);
            }
            _ => unreachable!("wrong variant"),
        }
    }

    #[test]
    fn all_protocol_messages_roundtrip() {
        let messages = vec![
            ProtocolMessage::HandshakeAck {
                success: true,
                error: None,
            },
            ProtocolMessage::Heartbeat { timestamp: 12345 },
            ProtocolMessage::MessageRelay {
                from: "a".into(),
                to: "b".into(),
                payload: "hello".into(),
            },
            ProtocolMessage::Broadcast {
                from: "a".into(),
                channel: "news".into(),
                payload: "update".into(),
            },
            ProtocolMessage::Disconnect {
                reason: "shutdown".into(),
            },
            ProtocolMessage::Error {
                message: "oops".into(),
            },
        ];
        for msg in messages {
            let json = serde_json::to_string(&msg).unwrap();
            let _: ProtocolMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn validate_key_accepts_valid() {
        let reg = RemoteAgentRegistry::new(test_config());
        assert!(reg.validate_key("test-key-123"));
        assert!(!reg.validate_key("wrong-key"));
    }

    #[test]
    fn validate_key_open_when_no_keys() {
        let config = RemoteConfig {
            api_keys: vec![],
            ..Default::default()
        };
        let reg = RemoteAgentRegistry::new(config);
        assert!(reg.validate_key("anything"));
    }

    #[tokio::test]
    async fn register_and_list() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        reg.register(
            "agent-1".into(),
            vec!["cap-a".into()],
            "ws://localhost:3000".into(),
            tx,
        )
        .await
        .unwrap();

        let agents = reg.list().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "agent-1");
        assert!(reg.is_connected("agent-1").await);
    }

    #[tokio::test]
    async fn register_duplicate_fails() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx1, _) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, _) = tokio::sync::mpsc::unbounded_channel();

        reg.register("dup".into(), vec![], "ws://a".into(), tx1)
            .await
            .unwrap();
        let err = reg
            .register("dup".into(), vec![], "ws://b".into(), tx2)
            .await
            .unwrap_err();
        assert!(err.contains("already connected"));
    }

    #[tokio::test]
    async fn unregister_removes_agent() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("agent-x".into(), vec![], "ws://x".into(), tx)
            .await
            .unwrap();
        assert!(reg.is_connected("agent-x").await);

        reg.unregister("agent-x").await;
        assert!(!reg.is_connected("agent-x").await);
        assert!(reg.list().await.is_empty());
    }

    #[tokio::test]
    async fn send_to_connected_agent() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        reg.register("agent-z".into(), vec![], "ws://z".into(), tx)
            .await
            .unwrap();

        let msg = ProtocolMessage::MessageRelay {
            from: "local".into(),
            to: "agent-z".into(),
            payload: "test".into(),
        };
        assert!(reg.send_to("agent-z", msg).await);
        let received = rx.recv().await.unwrap();
        match received {
            ProtocolMessage::MessageRelay { payload, .. } => {
                assert_eq!(payload, "test");
            }
            _ => unreachable!("wrong message type"),
        }
    }

    #[tokio::test]
    async fn send_to_disconnected_returns_false() {
        let reg = RemoteAgentRegistry::new(test_config());
        let msg = ProtocolMessage::Heartbeat { timestamp: 0 };
        assert!(!reg.send_to("ghost", msg).await);
    }

    #[test]
    fn remote_agent_staleness() {
        let agent = RemoteAgent {
            name: "test".into(),
            capabilities: vec![],
            connected_at: Instant::now(),
            last_heartbeat: Instant::now() - Duration::from_secs(100),
            endpoint: "ws://test".into(),
        };
        assert!(agent.is_stale(Duration::from_secs(90)));
        assert!(!agent.is_stale(Duration::from_secs(200)));
    }

    #[test]
    fn config_accessors_return_correct_durations() {
        let config = RemoteConfig {
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            api_keys: vec![],
        };
        let reg = RemoteAgentRegistry::new(config);
        assert_eq!(reg.heartbeat_interval(), Duration::from_secs(30));
        assert_eq!(reg.heartbeat_timeout(), Duration::from_secs(90));
    }

    #[tokio::test]
    async fn touch_heartbeat_keeps_agent_registered() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("hb-agent".into(), vec![], "ws://hb".into(), tx)
            .await
            .unwrap();

        // Touch should not panic and agent should remain connected.
        reg.touch_heartbeat("hb-agent").await;
        assert!(reg.is_connected("hb-agent").await);
    }

    #[tokio::test]
    async fn touch_heartbeat_unknown_agent_is_noop() {
        let reg = RemoteAgentRegistry::new(test_config());
        // Touching an agent that was never registered should not panic.
        reg.touch_heartbeat("nonexistent").await;
        assert!(!reg.is_connected("nonexistent").await);
    }

    #[tokio::test]
    async fn reap_stale_removes_timed_out_agents() {
        let config = RemoteConfig {
            heartbeat_timeout_secs: 0,
            ..Default::default()
        };
        let reg = RemoteAgentRegistry::new(config);
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("stale".into(), vec![], "ws://s".into(), tx)
            .await
            .unwrap();

        // Give elapsed() > Duration::ZERO a moment to become true.
        tokio::time::sleep(Duration::from_millis(1)).await;

        let reaped = reg.reap_stale().await;
        assert!(reaped.contains(&"stale".to_string()));
        assert!(!reg.is_connected("stale").await);
    }

    #[tokio::test]
    async fn reap_stale_keeps_fresh_agents() {
        let config = RemoteConfig {
            heartbeat_timeout_secs: 3600,
            ..Default::default()
        };
        let reg = RemoteAgentRegistry::new(config);
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("fresh".into(), vec![], "ws://f".into(), tx)
            .await
            .unwrap();

        let reaped = reg.reap_stale().await;
        assert!(reaped.is_empty());
        assert!(reg.is_connected("fresh").await);
    }

    #[tokio::test]
    async fn reap_stale_only_removes_stale_subset() {
        let config = RemoteConfig {
            heartbeat_timeout_secs: 0,
            ..Default::default()
        };
        let reg = RemoteAgentRegistry::new(config);

        let (tx1, _) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("will-reap".into(), vec![], "ws://r".into(), tx1)
            .await
            .unwrap();

        // Sleep so the first agent becomes stale.
        tokio::time::sleep(Duration::from_millis(1)).await;

        reg.register("just-joined".into(), vec![], "ws://j".into(), tx2)
            .await
            .unwrap();

        // The first agent is stale; the second was registered after the sleep.
        // With 0-second timeout both could be reaped depending on timing, but
        // the test verifies that reap_stale runs without error and removes stale entries.
        let reaped = reg.reap_stale().await;
        assert!(reaped.contains(&"will-reap".to_string()));
        // Regardless of timing for "just-joined", "will-reap" must be gone.
        assert!(!reg.is_connected("will-reap").await);
    }

    #[tokio::test]
    async fn register_multiple_agents() {
        let reg = RemoteAgentRegistry::new(test_config());
        for i in 0..5 {
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            reg.register(
                format!("agent-{i}"),
                vec![format!("cap-{i}")],
                format!("ws://host:{}", 8000 + i),
                tx,
            )
            .await
            .unwrap();
        }
        let agents = reg.list().await;
        assert_eq!(agents.len(), 5);
        for i in 0..5 {
            assert!(reg.is_connected(&format!("agent-{i}")).await);
        }
    }

    #[test]
    fn handshake_ack_error_roundtrip() {
        let msg = ProtocolMessage::HandshakeAck {
            success: false,
            error: Some("invalid api key".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"handshake_ack\""));
        assert!(json.contains("invalid api key"));
        let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ProtocolMessage::HandshakeAck {
                success,
                error: Some(e),
            } => {
                assert!(!success);
                assert_eq!(e, "invalid api key");
            }
            _ => unreachable!("wrong variant"),
        }
    }

    #[test]
    fn heartbeat_default_timestamp_is_nonzero() {
        // A Heartbeat with no explicit timestamp should use SystemTime::now().
        let json = r#"{"type":"heartbeat"}"#;
        let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
        match msg {
            ProtocolMessage::Heartbeat { timestamp } => {
                // The default_timestamp() function returns a real epoch second.
                // It will be > 0 unless the system clock is broken.
                assert!(timestamp > 0);
            }
            _ => unreachable!("wrong variant"),
        }
    }
}
