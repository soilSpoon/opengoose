use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};

use super::{ConnectionMetrics, ConnectionState, ProtocolMessage, RemoteAgent, RemoteConfig};

type ProtocolSender = tokio::sync::mpsc::UnboundedSender<ProtocolMessage>;

/// Central registry for all connected remote agents.
///
/// Thread-safe and clonable — share across handler tasks.
#[derive(Clone)]
pub struct RemoteAgentRegistry {
    agents: Arc<RwLock<HashMap<String, RemoteAgent>>>,
    config: Arc<RemoteConfig>,
    /// Channel for sending messages to remote agents.
    /// Key: agent name, Value: sender half of an unbounded channel.
    outbound: Arc<Mutex<HashMap<String, ProtocolSender>>>,
    /// Total number of agents that have connected since startup.
    total_connects: Arc<AtomicU64>,
    /// Total number of agents that have disconnected since startup.
    total_disconnects: Arc<AtomicU64>,
    /// Accumulated uptime seconds from all completed sessions.
    total_uptime_secs: Arc<AtomicU64>,
}

impl RemoteAgentRegistry {
    /// Create a new registry with the given configuration.
    pub fn new(config: RemoteConfig) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            outbound: Arc::new(Mutex::new(HashMap::new())),
            total_connects: Arc::new(AtomicU64::new(0)),
            total_disconnects: Arc::new(AtomicU64::new(0)),
            total_uptime_secs: Arc::new(AtomicU64::new(0)),
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
        tx: ProtocolSender,
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
            connection_state: ConnectionState::Connected,
        };

        self.agents.write().await.insert(name.clone(), agent);
        self.outbound.lock().await.insert(name, tx);
        self.total_connects.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Remove a remote agent from the registry, accumulating its uptime.
    pub async fn unregister(&self, name: &str) {
        if let Some(agent) = self.agents.write().await.remove(name) {
            let uptime = agent.connected_at.elapsed().as_secs();
            self.total_uptime_secs.fetch_add(uptime, Ordering::Relaxed);
        }
        self.outbound.lock().await.remove(name);
        self.total_disconnects.fetch_add(1, Ordering::Relaxed);
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
            if let Some(agent) = agents.remove(name) {
                let uptime = agent.connected_at.elapsed().as_secs();
                self.total_uptime_secs.fetch_add(uptime, Ordering::Relaxed);
            }
        }
        drop(agents);

        let mut outbound = self.outbound.lock().await;
        for name in &stale {
            outbound.remove(name);
        }
        drop(outbound);

        if !stale.is_empty() {
            self.total_disconnects
                .fetch_add(stale.len() as u64, Ordering::Relaxed);
        }

        stale
    }

    /// Transition a connected agent into `Reconnecting` state.
    ///
    /// Called when the client sends a `Reconnect` message. Does nothing if
    /// the agent is not currently registered.
    pub async fn mark_reconnecting(&self, name: &str) {
        if let Some(agent) = self.agents.write().await.get_mut(name) {
            agent.connection_state = ConnectionState::Reconnecting;
        }
    }

    /// Transition an agent back to `Connected` state after reconnect completes.
    pub async fn mark_connected(&self, name: &str) {
        if let Some(agent) = self.agents.write().await.get_mut(name) {
            agent.connection_state = ConnectionState::Connected;
            agent.last_heartbeat = Instant::now();
        }
    }

    /// Return aggregate connection metrics for the gateway health endpoint.
    pub async fn get_metrics(&self) -> ConnectionMetrics {
        let agents = self.agents.read().await;
        let active = agents.len();
        let total_connects = self.total_connects.load(Ordering::Relaxed);
        let total_disconnects = self.total_disconnects.load(Ordering::Relaxed);

        let accumulated = self.total_uptime_secs.load(Ordering::Relaxed);
        let active_uptime: u64 = agents
            .values()
            .map(|a| a.connected_at.elapsed().as_secs())
            .sum();
        let total_uptime = accumulated + active_uptime;

        let avg_uptime_secs = if total_connects > 0 {
            total_uptime / total_connects
        } else {
            0
        };

        ConnectionMetrics {
            active_connections: active,
            total_connects,
            total_disconnects,
            avg_uptime_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::RemoteAgentRegistry;
    use crate::remote::{ConnectionState, ProtocolMessage, RemoteConfig};

    fn test_config() -> RemoteConfig {
        RemoteConfig {
            heartbeat_interval_secs: 5,
            heartbeat_timeout_secs: 15,
            api_keys: vec!["test-key-123".to_string()],
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

        reg.touch_heartbeat("hb-agent").await;
        assert!(reg.is_connected("hb-agent").await);
    }

    #[tokio::test]
    async fn touch_heartbeat_unknown_agent_is_noop() {
        let reg = RemoteAgentRegistry::new(test_config());
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

        tokio::time::sleep(Duration::from_millis(1)).await;

        reg.register("just-joined".into(), vec![], "ws://j".into(), tx2)
            .await
            .unwrap();

        let reaped = reg.reap_stale().await;
        assert!(reaped.contains(&"will-reap".to_string()));
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

    #[tokio::test]
    async fn get_metrics_empty_registry() {
        let reg = RemoteAgentRegistry::new(test_config());
        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connects, 0);
        assert_eq!(metrics.total_disconnects, 0);
        assert_eq!(metrics.avg_uptime_secs, 0);
    }

    #[tokio::test]
    async fn get_metrics_tracks_connects_and_disconnects() {
        let reg = RemoteAgentRegistry::new(test_config());

        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("a".into(), vec![], "ws://a".into(), tx)
            .await
            .unwrap();

        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.active_connections, 1);
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 0);

        reg.unregister("a").await;
        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 1);
    }

    #[tokio::test]
    async fn get_metrics_avg_uptime_is_nonzero_after_disconnect() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("b".into(), vec![], "ws://b".into(), tx)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;
        reg.unregister("b").await;

        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.total_connects, 1);
        assert_eq!(metrics.total_disconnects, 1);
        let _ = metrics.avg_uptime_secs;
    }

    #[tokio::test]
    async fn reap_stale_updates_disconnect_metrics() {
        let config = RemoteConfig {
            heartbeat_timeout_secs: 0,
            ..Default::default()
        };
        let reg = RemoteAgentRegistry::new(config);
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("c".into(), vec![], "ws://c".into(), tx)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(1)).await;
        let reaped = reg.reap_stale().await;
        assert!(reaped.contains(&"c".to_string()));

        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.total_disconnects, 1);
        assert_eq!(metrics.active_connections, 0);
    }

    #[tokio::test]
    async fn mark_reconnecting_and_connected_transitions() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register("d".into(), vec![], "ws://d".into(), tx)
            .await
            .unwrap();

        {
            let agents = reg.agents.read().await;
            assert_eq!(agents["d"].connection_state, ConnectionState::Connected);
        }

        reg.mark_reconnecting("d").await;
        {
            let agents = reg.agents.read().await;
            assert_eq!(agents["d"].connection_state, ConnectionState::Reconnecting);
        }

        reg.mark_connected("d").await;
        {
            let agents = reg.agents.read().await;
            assert_eq!(agents["d"].connection_state, ConnectionState::Connected);
        }
    }

    #[tokio::test]
    async fn send_to_dropped_receiver_returns_false() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        reg.register("dropped".into(), vec![], "ws://d".into(), tx)
            .await
            .unwrap();

        drop(rx);

        let msg = ProtocolMessage::Heartbeat { timestamp: 1 };
        assert!(!reg.send_to("dropped", msg).await);
    }

    #[tokio::test]
    async fn mark_reconnecting_noop_for_unknown_agent() {
        let reg = RemoteAgentRegistry::new(test_config());
        reg.mark_reconnecting("ghost").await;
        assert!(!reg.is_connected("ghost").await);
    }

    #[tokio::test]
    async fn mark_connected_noop_for_unknown_agent() {
        let reg = RemoteAgentRegistry::new(test_config());
        reg.mark_connected("ghost").await;
        assert!(!reg.is_connected("ghost").await);
    }

    #[tokio::test]
    async fn unregister_nonexistent_agent_is_noop() {
        let reg = RemoteAgentRegistry::new(test_config());
        reg.unregister("never-existed").await;
        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.total_disconnects, 1);
    }

    #[tokio::test]
    async fn capabilities_are_stored_on_registration() {
        let reg = RemoteAgentRegistry::new(test_config());
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let caps = vec!["code-review".to_string(), "deploy".to_string()];
        reg.register("cap-agent".into(), caps.clone(), "ws://cap".into(), tx)
            .await
            .unwrap();

        let agents = reg.list().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].capabilities, caps);
    }

    #[tokio::test]
    async fn reap_stale_empty_registry_returns_empty() {
        let reg = RemoteAgentRegistry::new(test_config());
        let reaped = reg.reap_stale().await;
        assert!(reaped.is_empty());
    }

    #[tokio::test]
    async fn concurrent_registrations_all_succeed() {
        let reg = RemoteAgentRegistry::new(test_config());
        let reg = Arc::new(reg);

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let reg = reg.clone();
                tokio::spawn(async move {
                    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
                    reg.register(
                        format!("concurrent-{i}"),
                        vec![],
                        format!("ws://host:{}", 9000 + i),
                        tx,
                    )
                    .await
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let agents = reg.list().await;
        assert_eq!(agents.len(), 10);
        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.total_connects, 10);
    }

    #[tokio::test]
    async fn get_metrics_avg_uptime_across_multiple_disconnects() {
        let reg = RemoteAgentRegistry::new(test_config());

        for i in 0..3 {
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            reg.register(format!("agent-{i}"), vec![], format!("ws://{i}"), tx)
                .await
                .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(10)).await;

        for i in 0..3 {
            reg.unregister(&format!("agent-{i}")).await;
        }

        let metrics = reg.get_metrics().await;
        assert_eq!(metrics.total_connects, 3);
        assert_eq!(metrics.total_disconnects, 3);
        assert_eq!(metrics.active_connections, 0);
        let _ = metrics.avg_uptime_secs;
    }
}
