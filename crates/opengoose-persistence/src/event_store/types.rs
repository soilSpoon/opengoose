use opengoose_types::AppEventKind;

use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{EventHistoryRow, NewEventHistory};

const DEFAULT_HISTORY_LIMIT: i64 = 50;

#[derive(Clone, PartialEq)]
pub struct EventHistoryEntry {
    pub id: i32,
    pub event_kind: String,
    pub timestamp: String,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub payload: serde_json::Value,
}

impl std::fmt::Debug for EventHistoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHistoryEntry")
            .field("id", &self.id)
            .field("event_kind", &self.event_kind)
            .field("timestamp", &self.timestamp)
            .field("source_gateway", &self.source_gateway)
            .field("session_key", &"<redacted>")
            .field("payload", &"<redacted>")
            .finish()
    }
}

impl TryFrom<EventHistoryRow> for EventHistoryEntry {
    type Error = PersistenceError;

    fn try_from(row: EventHistoryRow) -> Result<Self, Self::Error> {
        let payload = serde_json::from_str(&row.payload)
            .map_err(|err| PersistenceError::Serialization(err.to_string()))?;

        Ok(Self {
            id: row.id,
            event_kind: row.event_kind,
            timestamp: row.timestamp,
            source_gateway: row.source_gateway,
            session_key: row.session_key,
            payload,
        })
    }
}

impl EventHistoryEntry {
    pub fn to_app_event_kind(&self) -> PersistenceResult<AppEventKind> {
        serde_json::from_value(self.payload.clone())
            .map_err(|err| PersistenceError::Serialization(err.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventHistoryQuery {
    pub limit: i64,
    pub offset: i64,
    pub event_kind: Option<String>,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub since: Option<String>,
}

impl Default for EventHistoryQuery {
    fn default() -> Self {
        Self {
            limit: DEFAULT_HISTORY_LIMIT,
            offset: 0,
            event_kind: None,
            source_gateway: None,
            session_key: None,
            since: None,
        }
    }
}

pub(super) struct NewStoredEvent {
    event_kind: String,
    source_gateway: Option<String>,
    session_key: Option<String>,
    payload: String,
}

impl NewStoredEvent {
    pub(super) fn from_kind(kind: &AppEventKind) -> PersistenceResult<Self> {
        let payload = serde_json::to_string(kind)
            .map_err(|err| PersistenceError::Serialization(err.to_string()))?;

        Ok(Self {
            event_kind: kind.key().to_owned(),
            source_gateway: kind.source_gateway().map(str::to_owned),
            session_key: kind.session_key().map(|key| key.to_stable_id()),
            payload,
        })
    }

    pub(super) fn as_insertable(&self) -> NewEventHistory<'_> {
        NewEventHistory {
            event_kind: &self.event_kind,
            source_gateway: self.source_gateway.as_deref(),
            session_key: self.session_key.as_deref(),
            payload: &self.payload,
        }
    }
}
