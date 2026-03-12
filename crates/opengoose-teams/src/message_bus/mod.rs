/// In-memory event bus for real-time inter-agent messaging.
///
/// Provides two communication patterns that complement the persistent
/// `AgentMessageStore`:
///
/// - **Directed**: point-to-point messages sent to a specific agent name.
/// - **Channel**: pub/sub messages published to a named topic; all current
///   subscribers receive a copy.
///
/// The bus is purely in-memory using `tokio::sync::broadcast`. For durable
/// delivery across process restarts use `AgentMessageStore` in parallel.
mod event;
mod registry;

pub use event::BusEvent;

use std::sync::Arc;

use tokio::sync::broadcast;

use registry::SenderRegistry;

/// A sharable handle to the message bus.
///
/// Clone freely — all clones share the same underlying channels.
#[derive(Clone)]
pub struct MessageBus {
    inner: Arc<BusInner>,
}

struct BusInner {
    /// Broadcast channel that receives *all* messages (directed + channel).
    /// Capacity chosen to be large enough for a typical multi-agent session.
    global_tx: broadcast::Sender<BusEvent>,
    registry: SenderRegistry,
}

impl MessageBus {
    /// Create a new message bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (global_tx, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(BusInner {
                global_tx,
                registry: SenderRegistry::new(capacity),
            }),
        }
    }

    /// Send a directed message to a specific agent.
    ///
    /// Returns the number of active receivers that got the message.
    pub fn send_directed(&self, from: &str, to: &str, payload: &str) -> usize {
        self.dispatch_event(BusEvent::directed(from, to, payload), |registry, event| {
            registry.send_directed(to, event)
        })
    }

    /// Publish a message to a named channel.
    ///
    /// Returns the number of active channel subscribers that received it.
    pub fn publish(&self, from: &str, channel: &str, payload: &str) -> usize {
        self.dispatch_event(
            BusEvent::channel(from, channel, payload),
            |registry, event| registry.publish(channel, event),
        )
    }

    /// Subscribe to directed messages for a specific agent.
    ///
    /// Returns a `broadcast::Receiver` that yields `BusEvent` values
    /// addressed to `agent_name`.
    pub fn subscribe_agent(&self, agent_name: &str) -> broadcast::Receiver<BusEvent> {
        self.inner.registry.subscribe_agent(agent_name)
    }

    /// Subscribe to a named channel (pub/sub).
    ///
    /// Returns a `broadcast::Receiver` that yields `BusEvent` values
    /// published to `channel_name`.
    pub fn subscribe_channel(&self, channel_name: &str) -> broadcast::Receiver<BusEvent> {
        self.inner.registry.subscribe_channel(channel_name)
    }

    /// Subscribe to all messages on the bus (global tap).
    pub fn subscribe_all(&self) -> broadcast::Receiver<BusEvent> {
        self.inner.global_tx.subscribe()
    }

    fn dispatch_event(
        &self,
        event: BusEvent,
        deliver: impl FnOnce(&SenderRegistry, BusEvent) -> usize,
    ) -> usize {
        let delivered = deliver(&self.inner.registry, event.clone());
        // The global tap must always observe the full event stream even when
        // there are no named subscribers for the targeted route.
        let _ = self.inner.global_tx.send(event);
        delivered
    }
}

#[cfg(test)]
mod tests;
