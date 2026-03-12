use diesel::prelude::*;

use crate::schema::event_history;

#[derive(Queryable, Selectable, Clone)]
#[diesel(table_name = event_history)]
pub struct EventHistoryRow {
    pub id: i32,
    pub event_kind: String,
    pub timestamp: String,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub payload: String,
}

impl std::fmt::Debug for EventHistoryRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHistoryRow")
            .field("id", &self.id)
            .field("event_kind", &self.event_kind)
            .field("timestamp", &self.timestamp)
            .field("source_gateway", &self.source_gateway)
            .field("session_key", &"<redacted>")
            .field("payload", &"<redacted>")
            .finish()
    }
}

#[derive(Insertable)]
#[diesel(table_name = event_history)]
pub struct NewEventHistory<'a> {
    pub event_kind: &'a str,
    pub source_gateway: Option<&'a str>,
    pub session_key: Option<&'a str>,
    pub payload: &'a str,
}
