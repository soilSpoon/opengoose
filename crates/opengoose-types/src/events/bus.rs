use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use super::{AppEvent, AppEventKind};

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
    reliable_taps: Arc<Mutex<Vec<mpsc::UnboundedSender<AppEvent>>>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            reliable_taps: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn emit(&self, kind: AppEventKind) {
        let event = AppEvent {
            kind,
            timestamp: std::time::Instant::now(),
        };
        // Ignore error — means no subscribers
        let _ = self.tx.send(event.clone());

        // Reliable taps back audit-style consumers with an unbounded queue so
        // they do not lose events when the broadcast ring buffer overruns.
        if let Ok(mut taps) = self.reliable_taps.lock() {
            taps.retain(|tap| tap.send(event.clone()).is_ok());
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    pub fn subscribe_reliable(&self) -> mpsc::UnboundedReceiver<AppEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut taps) = self.reliable_taps.lock() {
            taps.push(tx);
        }
        rx
    }
}

#[cfg(test)]
mod tests {
    use crate::Platform;

    use super::*;

    #[tokio::test]
    async fn test_event_bus_emit_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
    }

    #[test]
    fn test_event_bus_no_subscribers_no_panic() {
        let bus = EventBus::new(16);
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(
            e1.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
        assert!(matches!(
            e2.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
    }

    #[tokio::test]
    async fn test_event_bus_reliable_subscription_receives_event() {
        let bus = EventBus::new(1);
        let mut rx = bus.subscribe_reliable();

        bus.emit(AppEventKind::GooseReady);

        let event = rx.recv().await.expect("event should arrive");
        assert_eq!(event.kind.key(), "goose_ready");
    }
}
