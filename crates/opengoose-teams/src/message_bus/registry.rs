use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::broadcast;

use super::event::BusEvent;

type NamedSenders = Mutex<HashMap<String, broadcast::Sender<BusEvent>>>;

pub(super) struct SenderRegistry {
    channels: NamedSenders,
    directed: NamedSenders,
    capacity: usize,
}

impl SenderRegistry {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
            directed: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    pub(super) fn send_directed(&self, agent_name: &str, event: BusEvent) -> usize {
        send_named(&self.directed, agent_name, event)
    }

    pub(super) fn publish(&self, channel_name: &str, event: BusEvent) -> usize {
        send_named(&self.channels, channel_name, event)
    }

    pub(super) fn subscribe_agent(&self, agent_name: &str) -> broadcast::Receiver<BusEvent> {
        subscribe_named(&self.directed, agent_name, self.capacity)
    }

    pub(super) fn subscribe_channel(&self, channel_name: &str) -> broadcast::Receiver<BusEvent> {
        subscribe_named(&self.channels, channel_name, self.capacity)
    }
}

fn send_named(senders: &NamedSenders, name: &str, event: BusEvent) -> usize {
    let senders = senders.lock().unwrap_or_else(|e| e.into_inner());
    senders
        .get(name)
        .map(|tx| tx.send(event).unwrap_or(0))
        .unwrap_or(0)
}

fn subscribe_named(
    senders: &NamedSenders,
    name: &str,
    capacity: usize,
) -> broadcast::Receiver<BusEvent> {
    let mut senders = senders.lock().unwrap_or_else(|e| e.into_inner());
    senders
        .entry(name.to_string())
        .or_insert_with(|| {
            let (tx, _) = broadcast::channel(capacity);
            tx
        })
        .subscribe()
}
