/// Central registry for all connected remote agents.
mod lifecycle;
mod messaging;
mod registration;
mod types;

pub use types::{RemoteAgent, RemoteConfig};

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::{Mutex, RwLock, watch};

use super::transport::AgentTransport;

/// Central registry for all connected remote agents.
///
/// Thread-safe and clonable — share across handler tasks.
#[derive(Clone)]
pub struct RemoteAgentRegistry {
    pub(super) agents: Arc<RwLock<HashMap<String, RemoteAgent>>>,
    pub(super) config: Arc<RemoteConfig>,
    /// Channel for sending messages to remote agents.
    /// Key: agent name, Value: live sender and replay state.
    pub(super) outbound: Arc<Mutex<HashMap<String, AgentTransport>>>,
    /// Total number of agents that have connected since startup.
    pub(super) total_connects: Arc<AtomicU64>,
    /// Total number of agents that have disconnected since startup.
    pub(super) total_disconnects: Arc<AtomicU64>,
    /// Accumulated uptime seconds from all completed sessions.
    pub(super) total_uptime_secs: Arc<AtomicU64>,
    /// Monotonic revision counter for meaningful registry changes.
    pub(super) change_tx: watch::Sender<u64>,
}

impl RemoteAgentRegistry {
    /// Create a new registry with the given configuration.
    pub fn new(config: RemoteConfig) -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            outbound: Arc::new(Mutex::new(HashMap::new())),
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

    pub(super) fn notify_change(&self) {
        self.change_tx.send_modify(|revision| *revision += 1);
    }
}
