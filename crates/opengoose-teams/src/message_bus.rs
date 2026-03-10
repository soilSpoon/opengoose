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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::broadcast;

/// A single message event propagated over the bus.
#[derive(Debug, Clone)]
pub struct BusEvent {
    /// Name of the sending agent.
    pub from: String,
    /// Recipient agent name (for directed messages), or `None` for channel messages.
    pub to: Option<String>,
    /// Channel name (for pub/sub), or `None` for directed messages.
    pub channel: Option<String>,
    /// Message payload.
    pub payload: String,
    /// Wall-clock timestamp (seconds since Unix epoch).
    pub timestamp: u64,
}

impl BusEvent {
    /// Create a new directed message event.
    pub fn directed(
        from: impl Into<String>,
        to: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: Some(to.into()),
            channel: None,
            payload: payload.into(),
            timestamp: unix_now(),
        }
    }

    /// Create a new channel (pub/sub) event.
    pub fn channel(
        from: impl Into<String>,
        channel: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: None,
            channel: Some(channel.into()),
            payload: payload.into(),
            timestamp: unix_now(),
        }
    }

    /// Returns true for directed (point-to-point) events.
    pub fn is_directed(&self) -> bool {
        self.to.is_some()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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
    /// Per-channel broadcast senders.
    channels: Mutex<HashMap<String, broadcast::Sender<BusEvent>>>,
    /// Per-agent-name broadcast senders for directed delivery.
    directed: Mutex<HashMap<String, broadcast::Sender<BusEvent>>>,
    /// Default channel capacity.
    capacity: usize,
}

impl MessageBus {
    /// Create a new message bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (global_tx, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(BusInner {
                global_tx,
                channels: Mutex::new(HashMap::new()),
                directed: Mutex::new(HashMap::new()),
                capacity,
            }),
        }
    }

    /// Send a directed message to a specific agent.
    ///
    /// Returns the number of active receivers that got the message.
    pub fn send_directed(&self, from: &str, to: &str, payload: &str) -> usize {
        let event = BusEvent::directed(from, to, payload);
        // Publish to the named agent's channel if any subscriber exists.
        let agent_count = {
            let directed = self.inner.directed.lock().unwrap();
            if let Some(tx) = directed.get(to) {
                tx.send(event.clone()).unwrap_or(0)
            } else {
                0
            }
        };
        // Always publish to the global channel.
        let _ = self.inner.global_tx.send(event);
        agent_count
    }

    /// Publish a message to a named channel.
    ///
    /// Returns the number of active channel subscribers that received it.
    pub fn publish(&self, from: &str, channel: &str, payload: &str) -> usize {
        let event = BusEvent::channel(from, channel, payload);
        let channel_count = {
            let channels = self.inner.channels.lock().unwrap();
            if let Some(tx) = channels.get(channel) {
                tx.send(event.clone()).unwrap_or(0)
            } else {
                0
            }
        };
        let _ = self.inner.global_tx.send(event);
        channel_count
    }

    /// Subscribe to directed messages for a specific agent.
    ///
    /// Returns a `broadcast::Receiver` that yields `BusEvent` values
    /// addressed to `agent_name`.
    pub fn subscribe_agent(&self, agent_name: &str) -> broadcast::Receiver<BusEvent> {
        let mut directed = self.inner.directed.lock().unwrap();
        directed
            .entry(agent_name.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.inner.capacity);
                tx
            })
            .subscribe()
    }

    /// Subscribe to a named channel (pub/sub).
    ///
    /// Returns a `broadcast::Receiver` that yields `BusEvent` values
    /// published to `channel_name`.
    pub fn subscribe_channel(&self, channel_name: &str) -> broadcast::Receiver<BusEvent> {
        let mut channels = self.inner.channels.lock().unwrap();
        channels
            .entry(channel_name.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.inner.capacity);
                tx
            })
            .subscribe()
    }

    /// Subscribe to all messages on the bus (global tap).
    pub fn subscribe_all(&self) -> broadcast::Receiver<BusEvent> {
        self.inner.global_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_directed_delivery() {
        let bus = MessageBus::new(16);
        let mut rx = bus.subscribe_agent("agent-b");

        bus.send_directed("agent-a", "agent-b", "hello");

        let event = rx.recv().await.unwrap();
        assert_eq!(event.from, "agent-a");
        assert_eq!(event.to.as_deref(), Some("agent-b"));
        assert_eq!(event.payload, "hello");
        assert!(event.is_directed());
    }

    #[tokio::test]
    async fn test_channel_delivery() {
        let bus = MessageBus::new(16);
        let mut rx = bus.subscribe_channel("news");

        bus.publish("agent-a", "news", "breaking news!");

        let event = rx.recv().await.unwrap();
        assert_eq!(event.from, "agent-a");
        assert_eq!(event.channel.as_deref(), Some("news"));
        assert_eq!(event.payload, "breaking news!");
        assert!(!event.is_directed());
    }

    #[tokio::test]
    async fn test_global_tap_receives_all() {
        let bus = MessageBus::new(32);
        let mut global = bus.subscribe_all();

        bus.send_directed("a", "b", "direct msg");
        bus.publish("a", "ch", "channel msg");

        let e1 = global.recv().await.unwrap();
        assert!(e1.is_directed());

        let e2 = global.recv().await.unwrap();
        assert!(!e2.is_directed());
    }

    #[tokio::test]
    async fn test_multiple_channel_subscribers() {
        let bus = MessageBus::new(16);
        let mut rx1 = bus.subscribe_channel("updates");
        let mut rx2 = bus.subscribe_channel("updates");

        bus.publish("src", "updates", "ping");

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.payload, e2.payload);
    }

    #[tokio::test]
    async fn test_no_cross_channel_leakage() {
        let bus = MessageBus::new(16);
        let mut rx_news = bus.subscribe_channel("news");
        let mut rx_alerts = bus.subscribe_channel("alerts");

        bus.publish("src", "alerts", "only alerts");

        // alerts channel should get it
        let e = rx_alerts.recv().await.unwrap();
        assert_eq!(e.payload, "only alerts");

        // news channel must not get it (try_recv returns Err immediately)
        assert!(rx_news.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_directed_no_cross_agent_leakage() {
        let bus = MessageBus::new(16);
        let mut rx_b = bus.subscribe_agent("agent-b");
        let mut rx_c = bus.subscribe_agent("agent-c");

        bus.send_directed("a", "agent-b", "for b only");

        let e = rx_b.recv().await.unwrap();
        assert_eq!(e.payload, "for b only");

        assert!(rx_c.try_recv().is_err());
    }

    #[test]
    fn test_bus_event_helpers() {
        let d = BusEvent::directed("a", "b", "payload");
        assert!(d.is_directed());
        assert_eq!(d.to.as_deref(), Some("b"));
        assert!(d.channel.is_none());

        let c = BusEvent::channel("a", "ch", "payload");
        assert!(!c.is_directed());
        assert_eq!(c.channel.as_deref(), Some("ch"));
        assert!(c.to.is_none());
    }

    #[test]
    fn test_send_directed_no_subscribers_returns_zero() {
        let bus = MessageBus::new(16);
        // No subscribers for "agent-x"
        let count = bus.send_directed("agent-a", "agent-x", "hello");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_publish_no_subscribers_returns_zero() {
        let bus = MessageBus::new(16);
        // No subscribers for "events" channel
        let count = bus.publish("agent-a", "events", "data");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_send_directed_with_subscriber_returns_one() {
        let bus = MessageBus::new(16);
        let _rx = bus.subscribe_agent("agent-b");

        let count = bus.send_directed("a", "agent-b", "msg");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_publish_with_subscriber_returns_count() {
        let bus = MessageBus::new(16);
        let _rx1 = bus.subscribe_channel("updates");
        let _rx2 = bus.subscribe_channel("updates");

        let count = bus.publish("src", "updates", "ping");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_bus_event_timestamp_is_nonzero() {
        let event = BusEvent::directed("a", "b", "msg");
        assert!(event.timestamp > 0, "timestamp should be set");
    }

    #[test]
    fn test_bus_clone_shares_state() {
        let bus1 = MessageBus::new(16);
        let bus2 = bus1.clone();
        let mut rx = bus1.subscribe_agent("agent-a");

        // Send on clone, receive on original's subscriber
        bus2.send_directed("sender", "agent-a", "via clone");

        let event = rx.try_recv().unwrap();
        assert_eq!(event.payload, "via clone");
    }
}
