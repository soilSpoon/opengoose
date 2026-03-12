mod queries;
mod recorder;
#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;

use diesel::prelude::*;
use opengoose_types::{AppEventKind, EventBus};
use tracing::info;

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::models::EventHistoryRow;
use crate::schema::event_history;

pub use queries::normalize_since_filter;
pub use recorder::{EventHistoryRecorderHandle, spawn_event_history_recorder};
pub use types::{EventHistoryEntry, EventHistoryQuery};

pub const DEFAULT_EVENT_RETENTION_DAYS: u32 = 30;

pub struct EventStore {
    db: Arc<Database>,
}

impl EventStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn record(&self, kind: &AppEventKind) -> PersistenceResult<EventHistoryEntry> {
        let new_event = types::NewStoredEvent::from_kind(kind)?;

        self.db.with(|conn| {
            let row = diesel::insert_into(event_history::table)
                .values(&new_event.as_insertable())
                .returning(EventHistoryRow::as_returning())
                .get_result(conn)?;
            EventHistoryEntry::try_from(row)
        })
    }

    pub fn list(&self, query: &EventHistoryQuery) -> PersistenceResult<Vec<EventHistoryEntry>> {
        self.db
            .with(|conn| queries::load_event_history(conn, query))
    }

    pub fn cleanup_expired(&self, retention_days: u32) -> PersistenceResult<usize> {
        let deleted = self
            .db
            .with(|conn| queries::cleanup_expired_events(conn, retention_days))?;

        if deleted > 0 {
            info!(deleted, retention_days, "cleaned up expired events");
        }

        Ok(deleted)
    }

    pub fn replay(
        &self,
        query: &EventHistoryQuery,
        event_bus: &EventBus,
    ) -> PersistenceResult<usize> {
        replay_entries(self.list(query)?, event_bus)
    }
}

fn replay_entries(
    entries: Vec<EventHistoryEntry>,
    event_bus: &EventBus,
) -> PersistenceResult<usize> {
    let mut replayed = 0usize;

    for entry in entries.into_iter().rev() {
        event_bus.emit(entry.to_app_event_kind()?);
        replayed += 1;
    }

    Ok(replayed)
}
