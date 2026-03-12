use std::sync::Arc;
use std::time::Duration as StdDuration;

use opengoose_types::{AppEvent, AppEventKind, EventBus};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::db::Database;

use super::EventStore;

enum RecorderCommand {
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

pub struct EventHistoryRecorderHandle {
    command_tx: mpsc::UnboundedSender<RecorderCommand>,
    join: tokio::task::JoinHandle<()>,
}

impl EventHistoryRecorderHandle {
    pub async fn flush(&self, timeout: StdDuration) -> bool {
        let (tx, rx) = oneshot::channel();
        if self.command_tx.send(RecorderCommand::Flush(tx)).is_err() {
            return false;
        }

        matches!(tokio::time::timeout(timeout, rx).await, Ok(Ok(())))
    }

    pub async fn shutdown(self, timeout: StdDuration) -> bool {
        let (tx, rx) = oneshot::channel();
        if self.command_tx.send(RecorderCommand::Shutdown(tx)).is_err() {
            return false;
        }

        let acked = matches!(tokio::time::timeout(timeout, rx).await, Ok(Ok(())));
        let joined = matches!(tokio::time::timeout(timeout, self.join).await, Ok(Ok(())));
        acked && joined
    }
}

fn record_event(store: &EventStore, kind: &AppEventKind) {
    if let Err(error) = store.record(kind) {
        warn!(%error, event_kind = kind.key(), "failed to persist event");
    }
}

fn drain_pending_events(store: &EventStore, rx: &mut mpsc::UnboundedReceiver<AppEvent>) {
    while let Ok(event) = rx.try_recv() {
        record_event(store, &event.kind);
    }
}

pub fn spawn_event_history_recorder(
    db: Arc<Database>,
    event_bus: EventBus,
) -> EventHistoryRecorderHandle {
    let store = EventStore::new(db);
    let mut rx = event_bus.subscribe_reliable();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();

    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                command = command_rx.recv() => match command {
                    Some(RecorderCommand::Flush(reply)) => {
                        drain_pending_events(&store, &mut rx);
                        let _ = reply.send(());
                    }
                    Some(RecorderCommand::Shutdown(reply)) => {
                        drain_pending_events(&store, &mut rx);
                        let _ = reply.send(());
                        break;
                    }
                    None => break,
                },
                maybe_event = rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };
                    record_event(&store, &event.kind);
                }
            }
        }
    });

    EventHistoryRecorderHandle { command_tx, join }
}
