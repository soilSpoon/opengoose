use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_types::EventBus;
use tracing::warn;

use super::LIVE_EVENT_POLL_INTERVAL;
use super::changes::emit_live_snapshot_changes;
use super::snapshot::{LiveSnapshot, capture_live_snapshot};

pub(crate) fn spawn_live_event_watcher(db: Arc<Database>, event_bus: EventBus) {
    tokio::spawn(async move {
        let mut snapshot = match capture_live_snapshot(db.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!(%error, "failed to capture initial live snapshot");
                LiveSnapshot::default()
            }
        };

        let mut ticker = tokio::time::interval(LIVE_EVENT_POLL_INTERVAL);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match capture_live_snapshot(db.clone()) {
                Ok(next) => {
                    emit_live_snapshot_changes(&snapshot, &next, &event_bus);
                    snapshot = next;
                }
                Err(error) => warn!(%error, "failed to refresh live snapshot"),
            }
        }
    });
}
