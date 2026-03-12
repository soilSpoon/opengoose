/// Central registry for all connected remote agents.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::watch;

use super::protocol::{ConnectionMetrics, ConnectionState, ProtocolMessage};
use super::transport::{AgentTransport, ReplayResult, should_buffer_for_replay};

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

/// Central registry for all connected remote agents.
///
/// Thread-safe and clonable — share across handler tasks.
#[derive(Clone)]
pub struct RemoteAgentRegistry {
    agents: Arc<DashMap<String, RemoteAgent>>,
    config: Arc<RemoteConfig>,
    /// Channel for sending messages to remote agents.
    /// Key: agent name, Value: live sender and replay state.
    outbound: Arc<DashMap<String, AgentTransport>>,
    /// Total number of agents that have connected since startup.
    total_connects: Arc<AtomicU64>,
    /// Total number of agents that have disconnected since startup.
    total_disconnects: Arc<AtomicU64>,
    /// Accumulated uptime seconds from all completed sessions.
    total_uptime_secs: Arc<AtomicU64>,
    /// Monotonic revision counter for meaningful registry changes.
    change_tx: watch::Sender<u64>,
}

impl RemoteAgentRegistry {
    /// Create a new registry with the given configuration.
    pub fn new(config: RemoteConfig) -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            agents: Arc::new(DashMap::new()),
            config: Arc::new(config),
            outbound: Arc::new(DashMap::new()),
            total_connects: Arc::new(AtomicU64::new(0)),
            total_disconnects: Arc::new(AtomicU64::new(0)),
            total_uptime_secs: Arc::new(AtomicU64::new(0)),
            change_tx,
        }
    }

    /// Subscribe to a monotonic revision counter that advances on meaningful
    /// registry state changes.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    fn notify_change(&self) {
        self.change_tx.send_modify(|revision| *revision += 1);
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
        if let Some(mut agent) = self.agents.get_mut(&name) {
            match self.outbound.get_mut(&name) {
                Some(mut transport) if transport.tx.is_none() => {
                    transport.attach(tx);
                    agent.capabilities = capabilities;
                    agent.endpoint = endpoint;
                    agent.last_heartbeat = Instant::now();
                    agent.connection_state = ConnectionState::Connected;
                    return Ok(());
                }
                Some(_) => return Err(format!("agent '{}' is already connected", name)),
                None => {
                    self.outbound.insert(name.clone(), AgentTransport::new(tx));
                    agent.capabilities = capabilities;
                    agent.endpoint = endpoint;
                    agent.last_heartbeat = Instant::now();
                    agent.connection_state = ConnectionState::Connected;
                    return Ok(());
                }
            }
        }

        let now = Instant::now();
        let agent = RemoteAgent {
            name: name.clone(),
            capabilities,
            connected_at: now,
            last_heartbeat: now,
            endpoint,
            connection_state: ConnectionState::Connected,
        };

        self.agents.insert(name.clone(), agent);
        self.outbound.insert(name, AgentTransport::new(tx));
        self.total_connects.fetch_add(1, Ordering::Relaxed);
        self.notify_change();
        Ok(())
    }

    /// Remove a remote agent from the registry, accumulating its uptime.
    pub async fn unregister(&self, name: &str) {
        let mut removed = false;
        if let Some((_, agent)) = self.agents.remove(name) {
            let uptime = agent.connected_at.elapsed().as_secs();
            self.total_uptime_secs.fetch_add(uptime, Ordering::Relaxed);
            removed = true;
        }
        if self.outbound.remove(name).is_some() {
            removed = true;
        }
        self.total_disconnects.fetch_add(1, Ordering::Relaxed);
        if removed {
            self.notify_change();
        }
    }

    /// Update the heartbeat timestamp for an agent.
    pub async fn touch_heartbeat(&self, name: &str) {
        let mut changed = false;
        if let Some(mut agent) = self.agents.get_mut(name) {
            agent.last_heartbeat = Instant::now();
            changed = true;
        }
        if changed {
            self.notify_change();
        }
    }

    /// Detach the live transport for an agent while preserving replay state.
    ///
    /// Used when the socket drops unexpectedly and the client may reconnect.
    pub async fn detach_connection(&self, name: &str) -> bool {
        let detached = if let Some(mut transport) = self.outbound.get_mut(name) {
            transport.detach();
            true
        } else {
            return false;
        };

        if detached && let Some(mut agent) = self.agents.get_mut(name) {
            agent.connection_state = ConnectionState::Reconnecting;
            agent.last_heartbeat = Instant::now();
        }

        detached
    }

    /// Remove an agent only if it is still detached when the reconnect grace expires.
    pub async fn unregister_if_detached(&self, name: &str) -> bool {
        let should_remove = matches!(self.outbound.get(name), Some(transport) if transport.tx.is_none());

        if should_remove {
            self.unregister(name).await;
        }

        should_remove
    }

    /// Send a protocol message to a specific remote agent.
    ///
    /// Returns `true` if the message was delivered immediately or buffered for replay.
    pub async fn send_to(&self, name: &str, msg: ProtocolMessage) -> bool {
        let bufferable = should_buffer_for_replay(&msg);
        let mut should_mark_reconnecting = false;

        let accepted = {
            let Some(mut transport) = self.outbound.get_mut(name) else {
                return false;
            };

            if bufferable {
                let event_id = transport.next_event_id;
                transport.next_event_id = transport.next_event_id.saturating_add(1);
                transport
                    .replay_buffer
                    .push_back(super::transport::ReplayEvent {
                        event_id,
                        message: msg.clone(),
                    });

                while transport.replay_buffer.len() > self.config.replay_buffer_capacity {
                    transport.replay_buffer.pop_front();
                }
            }

            match transport.tx.as_ref() {
                Some(tx) => {
                    if tx.send(msg).is_ok() {
                        true
                    } else {
                        transport.detach();
                        should_mark_reconnecting = true;
                        bufferable
                    }
                }
                None => bufferable,
            }
        };

        if should_mark_reconnecting {
            self.mark_reconnecting(name).await;
        }

        accepted
    }

    /// Re-enqueue buffered outbound events newer than `last_event_id`.
    pub async fn replay_since(&self, name: &str, last_event_id: u64) -> ReplayResult {
        let Some(mut transport) = self.outbound.get_mut(name) else {
            return ReplayResult::Unavailable;
        };

        let Some(tx) = transport.tx.as_ref() else {
            return ReplayResult::Unavailable;
        };

        let newest_event_id = transport.next_event_id.saturating_sub(1);
        if last_event_id > newest_event_id {
            return ReplayResult::BufferMiss;
        }
        if last_event_id == newest_event_id {
            return ReplayResult::Replayed(0);
        }

        let Some(oldest_event_id) = transport.replay_buffer.front().map(|event| event.event_id)
        else {
            return ReplayResult::BufferMiss;
        };
        if last_event_id.saturating_add(1) < oldest_event_id {
            return ReplayResult::BufferMiss;
        }

        let replayable: Vec<ProtocolMessage> = transport
            .replay_buffer
            .iter()
            .filter(|event| event.event_id > last_event_id)
            .map(|event| event.message.clone())
            .collect();

        let replayed_events = replayable.len() as u64;
        for message in replayable {
            if tx.send(message).is_err() {
                transport.detach();
                return ReplayResult::Unavailable;
            }
        }

        ReplayResult::Replayed(replayed_events)
    }

    /// List all currently connected remote agents.
    pub async fn list(&self) -> Vec<RemoteAgent> {
        self.agents.iter().map(|agent| agent.value().clone()).collect()
    }

    /// Check if a specific agent is connected.
    pub async fn is_connected(&self, name: &str) -> bool {
        self.agents.contains_key(name)
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
        let stale: Vec<String> = self
            .agents
            .iter()
            .filter(|entry| entry.value().is_stale(timeout))
            .map(|entry| entry.key().clone())
            .collect();

        for name in &stale {
            if let Some((_, agent)) = self.agents.remove(name) {
                let uptime = agent.connected_at.elapsed().as_secs();
                self.total_uptime_secs.fetch_add(uptime, Ordering::Relaxed);
            }
        }

        for name in &stale {
            self.outbound.remove(name);
        }

        if !stale.is_empty() {
            self.total_disconnects
                .fetch_add(stale.len() as u64, Ordering::Relaxed);
            self.notify_change();
        }

        stale
    }

    /// Transition a connected agent into `Reconnecting` state.
    ///
    /// Called when the client sends a `Reconnect` message. Does nothing if
    /// the agent is not currently registered.
    pub async fn mark_reconnecting(&self, name: &str) {
        let mut changed = false;
        if let Some(mut agent) = self.agents.get_mut(name)
            && agent.connection_state != ConnectionState::Reconnecting
        {
            agent.connection_state = ConnectionState::Reconnecting;
            changed = true;
        }
        if changed {
            self.notify_change();
        }
    }

    /// Transition an agent back to `Connected` state after reconnect completes.
    pub async fn mark_connected(&self, name: &str) {
        let mut changed = false;
        if let Some(mut agent) = self.agents.get_mut(name) {
            agent.connection_state = ConnectionState::Connected;
            agent.last_heartbeat = Instant::now();
            changed = true;
        }
        if changed {
            self.notify_change();
        }
    }

    /// Return aggregate connection metrics for the gateway health endpoint.
    pub async fn get_metrics(&self) -> ConnectionMetrics {
        let active = self.agents.len();
        let total_connects = self.total_connects.load(Ordering::Relaxed);
        let total_disconnects = self.total_disconnects.load(Ordering::Relaxed);

        let accumulated = self.total_uptime_secs.load(Ordering::Relaxed);
        let active_uptime: u64 = self
            .agents
            .iter()
            .map(|agent| agent.value().connected_at.elapsed().as_secs())
            .sum();
        let total_uptime = accumulated + active_uptime;

        let avg_uptime_secs = total_uptime.checked_div(total_connects).unwrap_or(0);

        ConnectionMetrics {
            active_connections: active,
            total_connects,
            total_disconnects,
            avg_uptime_secs,
        }
    }
}
