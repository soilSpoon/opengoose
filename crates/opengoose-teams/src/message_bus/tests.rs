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

    let e = rx_alerts.recv().await.unwrap();
    assert_eq!(e.payload, "only alerts");

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
    let count = bus.send_directed("agent-a", "agent-x", "hello");
    assert_eq!(count, 0);
}

#[test]
fn test_publish_no_subscribers_returns_zero() {
    let bus = MessageBus::new(16);
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

    bus2.send_directed("sender", "agent-a", "via clone");

    let event = rx.try_recv().unwrap();
    assert_eq!(event.payload, "via clone");
}
