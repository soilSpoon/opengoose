use std::sync::atomic::Ordering;
use std::time::Instant;

use super::super::protocol::{ConnectionState, ProtocolMessage};
use super::super::transport::AgentTransport;
use super::types::RemoteAgent;
use super::RemoteAgentRegistry;

impl RemoteAgentRegistry {
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
        let mut agents = self.agents.write().await;
        let mut outbound = self.outbound.lock().await;

        if let Some(agent) = agents.get_mut(&name) {
            match outbound.get_mut(&name) {
                Some(transport) if transport.tx.is_none() => {
                    transport.attach(tx);
                    agent.capabilities = capabilities;
                    agent.endpoint = endpoint;
                    agent.last_heartbeat = Instant::now();
                    agent.connection_state = ConnectionState::Connected;
                    return Ok(());
                }
                Some(_) => return Err(format!("agent '{}' is already connected", name)),
                None => {
                    outbound.insert(name.clone(), AgentTransport::new(tx));
                    agent.capabilities = capabilities;
                    agent.endpoint = endpoint;
                    agent.last_heartbeat = Instant::now();
                    agent.connection_state = ConnectionState::Connected;
                    return Ok(());
                }
            }
        }
        drop(outbound);
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
        self.outbound
            .lock()
            .await
            .insert(name, AgentTransport::new(tx));
        self.total_connects.fetch_add(1, Ordering::Relaxed);
        self.notify_change();
        Ok(())
    }

    /// Remove a remote agent from the registry, accumulating its uptime.
    pub async fn unregister(&self, name: &str) {
        let mut removed = false;
        if let Some(agent) = self.agents.write().await.remove(name) {
            let uptime = agent.connected_at.elapsed().as_secs();
            self.total_uptime_secs.fetch_add(uptime, Ordering::Relaxed);
            removed = true;
        }
        if self.outbound.lock().await.remove(name).is_some() {
            removed = true;
        }
        self.total_disconnects.fetch_add(1, Ordering::Relaxed);
        if removed {
            self.notify_change();
        }
    }
}
