use std::sync::Arc;

use opengoose_persistence::Database;
use opengoose_types::{AppEvent, AppEventKind};

fn make_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("in-memory db"))
}

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<AppEventKind> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event.kind);
    }
    events
}

mod event_changes;
mod snapshot_capture;
