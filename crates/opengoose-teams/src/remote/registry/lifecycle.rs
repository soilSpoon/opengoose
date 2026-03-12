use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::super::protocol::{ConnectionMetrics, ConnectionState};
use super::RemoteAgentRegistry;
use super::types::RemoteAgent;

impl RemoteAgentRegistry {
    /// Update the heartbeat timestamp for an agent.
    pub async fn touch_heartbeat(&self, name: &str) {
        let mut changed = false;
        if let Some(agent) = self.agents.write().await.get_mut(name) {
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
        let detached = {
            let mut outbound = self.outbound.lock().await;
            let Some(transport) = outbound.get_mut(name) else {
                return false;
            };
            transport.detach();
            true
        };

        if detached && let Some(agent) = self.agents.write().await.get_mut(name) {
            agent.connection_state = ConnectionState::Reconnecting;
            agent.last_heartbeat = Instant::now();
        }

        detached
    }

    /// Remove an agent only if it is still detached when the reconnect grace expires.
    pub async fn unregister_if_detached(&self, name: &str) -> bool {
        let should_remove = {
            let outbound = self.outbound.lock().await;
            matches!(outbound.get(name), Some(transport) if transport.tx.is_none())
        };

        if should_remove {
            self.unregister(name).await;
        }

        should_remove
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
        if let Some(agent) = self.agents.write().await.get_mut(name)
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
        if let Some(agent) = self.agents.write().await.get_mut(name) {
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

        let avg_uptime_secs = total_uptime.checked_div(total_connects).unwrap_or(0);

        ConnectionMetrics {
            active_connections: active,
            total_connects,
            total_disconnects,
            avg_uptime_secs,
        }
    }
}
