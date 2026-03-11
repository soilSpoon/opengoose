use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::{broadcast, mpsc};

use super::AppEventKind;

#[derive(Debug, Clone)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub timestamp: Instant,
}

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
            timestamp: Instant::now(),
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
