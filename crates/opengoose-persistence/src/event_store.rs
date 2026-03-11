use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::sql_types::Text;
use opengoose_types::{AppEventKind, EventBus};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::db::Database;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{EventHistoryRow, NewEventHistory};
use crate::schema::event_history;

pub const DEFAULT_EVENT_RETENTION_DAYS: u32 = 30;
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

pub struct EventStore {
    db: Arc<Database>,
}

impl EventStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn record(&self, kind: &AppEventKind) -> PersistenceResult<EventHistoryEntry> {
        let payload = serde_json::to_string(kind)
            .map_err(|err| PersistenceError::Serialization(err.to_string()))?;
        let session_key = kind.session_key().map(|key| key.to_stable_id());
        let new_event = NewEventHistory {
            event_kind: kind.key(),
            source_gateway: kind.source_gateway(),
            session_key: session_key.as_deref(),
            payload: &payload,
        };

        self.db.with(|conn| {
            let row = diesel::insert_into(event_history::table)
                .values(&new_event)
                .returning(EventHistoryRow::as_returning())
                .get_result(conn)?;
            EventHistoryEntry::try_from(row)
        })
    }

    pub fn list(&self, query: &EventHistoryQuery) -> PersistenceResult<Vec<EventHistoryEntry>> {
        self.db.with(|conn| {
            let mut statement = event_history::table.into_boxed::<diesel::sqlite::Sqlite>();

            if let Some(value) = query.event_kind.as_deref() {
                statement = statement.filter(event_history::event_kind.eq(value));
            }
            if let Some(value) = query.source_gateway.as_deref() {
                statement = statement.filter(event_history::source_gateway.eq(Some(value)));
            }
            if let Some(value) = query.session_key.as_deref() {
                statement = statement.filter(event_history::session_key.eq(Some(value)));
            }
            if let Some(value) = query.since.as_deref() {
                statement = statement.filter(event_history::timestamp.ge(value));
            }

            let rows = statement
                .order((event_history::timestamp.desc(), event_history::id.desc()))
                .offset(query.offset)
                .limit(query.limit)
                .select(EventHistoryRow::as_select())
                .load(conn)?;

            rows.into_iter()
                .map(EventHistoryEntry::try_from)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    pub fn cleanup_expired(&self, retention_days: u32) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let cutoff = format!("-{retention_days} days");
            let deleted = diesel::sql_query(
                "DELETE FROM event_history WHERE timestamp < datetime('now', ?1)",
            )
            .bind::<Text, _>(&cutoff)
            .execute(conn)?;

            if deleted > 0 {
                info!(deleted, retention_days, "cleaned up expired events");
            }

            Ok(deleted)
        })
    }

    pub fn replay(
        &self,
        query: &EventHistoryQuery,
        event_bus: &EventBus,
    ) -> PersistenceResult<usize> {
        let entries = self.list(query)?;
        let mut replayed = 0usize;

        for entry in entries.into_iter().rev() {
            event_bus.emit(entry.to_app_event_kind()?);
            replayed += 1;
        }

        Ok(replayed)
    }
}

pub fn spawn_event_history_recorder(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
) {
    let store = EventStore::new(db);
    let mut rx = event_bus.subscribe_reliable();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                maybe_event = rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };

                    if let Err(error) = store.record(&event.kind) {
                        warn!(%error, event_kind = event.kind.key(), "failed to persist event");
                    }
                }
            }
        }
    });
}

pub fn normalize_since_filter(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("`since` must not be empty".into());
    }

    if let Some(relative) = parse_relative_since(trimmed)? {
        return Ok(relative.format("%Y-%m-%d %H:%M:%S").to_string());
    }

    if let Ok(timestamp) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(timestamp
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string());
    }

    if let Ok(timestamp) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return Ok(timestamp.format("%Y-%m-%d %H:%M:%S").to_string());
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(date
            .and_hms_opt(0, 0, 0)
            .expect("midnight should be valid")
            .format("%Y-%m-%d %H:%M:%S")
            .to_string());
    }

    Err(format!(
        "unsupported `since` value `{trimmed}`; use values like `24h`, `7d`, RFC3339, or `YYYY-MM-DD HH:MM:SS`"
    ))
}

fn parse_relative_since(raw: &str) -> Result<Option<DateTime<Utc>>, String> {
    let Some(unit) = raw.chars().last() else {
        return Ok(None);
    };

    if !matches!(unit, 's' | 'm' | 'h' | 'd' | 'w') {
        return Ok(None);
    }

    let value = raw[..raw.len() - 1]
        .parse::<i64>()
        .map_err(|_| format!("invalid relative `since` value `{raw}`"))?;

    let duration = match unit {
        's' => Duration::seconds(value),
        'm' => Duration::minutes(value),
        'h' => Duration::hours(value),
        'd' => Duration::days(value),
        'w' => Duration::weeks(value),
        _ => unreachable!("validated above"),
    };

    Ok(Some(Utc::now() - duration))
}

#[cfg(test)]
mod tests {
    use std::time::Duration as StdDuration;

    use tokio::time::{sleep, timeout};

    use super::*;
    use crate::Database;
    use opengoose_types::{Platform, SessionKey};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("in-memory db should open"))
    }

    #[test]
    fn record_and_list_roundtrip() {
        let store = EventStore::new(test_db());

        store
            .record(&AppEventKind::MessageReceived {
                session_key: SessionKey::new(Platform::Discord, "ops", "bridge"),
                author: "alice".into(),
                content: "hello".into(),
            })
            .expect("event should be recorded");

        let entries = store
            .list(&EventHistoryQuery::default())
            .expect("history should load");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_kind, "message_received");
        assert_eq!(entries[0].source_gateway.as_deref(), Some("discord"));
        assert_eq!(
            entries[0].session_key.as_deref(),
            Some("discord:ns:ops:bridge")
        );
        assert_eq!(entries[0].payload["type"], "message_received");
    }

    #[test]
    fn list_filters_by_gateway_and_kind() {
        let store = EventStore::new(test_db());

        store
            .record(&AppEventKind::GooseReady)
            .expect("goose event should persist");
        store
            .record(&AppEventKind::ChannelReady {
                platform: Platform::Slack,
            })
            .expect("channel event should persist");

        let entries = store
            .list(&EventHistoryQuery {
                source_gateway: Some("slack".into()),
                event_kind: Some("channel_ready".into()),
                ..EventHistoryQuery::default()
            })
            .expect("filtered history should load");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_kind, "channel_ready");
        assert_eq!(entries[0].source_gateway.as_deref(), Some("slack"));
    }

    #[test]
    fn cleanup_expired_deletes_old_events() {
        let db = test_db();
        let store = EventStore::new(db.clone());

        let event = store
            .record(&AppEventKind::GooseReady)
            .expect("event should persist");

        db.with(|conn| {
            diesel::sql_query(
                "UPDATE event_history SET timestamp = datetime('now', '-40 days') WHERE id = ?1",
            )
            .bind::<diesel::sql_types::Integer, _>(event.id)
            .execute(conn)?;
            Ok(())
        })
        .expect("timestamp update should succeed");

        let deleted = store.cleanup_expired(30).expect("cleanup should succeed");

        assert_eq!(deleted, 1);
        assert!(
            store
                .list(&EventHistoryQuery::default())
                .expect("history should load")
                .is_empty()
        );
    }

    #[test]
    fn replay_reemits_persisted_events() {
        let store = EventStore::new(test_db());
        let replay_bus = EventBus::new(8);
        let mut rx = replay_bus.subscribe();

        store
            .record(&AppEventKind::ChannelReady {
                platform: Platform::Discord,
            })
            .expect("event should persist");

        let replayed = store
            .replay(&EventHistoryQuery::default(), &replay_bus)
            .expect("replay should succeed");
        let replayed_event = rx.try_recv().expect("event should be replayed");

        assert_eq!(replayed, 1);
        assert!(matches!(
            replayed_event.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
    }

    #[tokio::test]
    async fn recorder_persists_events_from_reliable_tap() {
        let db = test_db();
        let store = EventStore::new(db.clone());
        let bus = EventBus::new(1);
        let cancel = CancellationToken::new();

        spawn_event_history_recorder(db, bus.clone(), cancel.clone());
        bus.emit(AppEventKind::GooseReady);

        timeout(StdDuration::from_secs(1), async {
            loop {
                if let Ok(entries) = store.list(&EventHistoryQuery::default())
                    && !entries.is_empty()
                {
                    break;
                }
                sleep(StdDuration::from_millis(10)).await;
            }
        })
        .await
        .expect("event should be recorded");

        cancel.cancel();

        let entries = store
            .list(&EventHistoryQuery::default())
            .expect("history should load");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_kind, "goose_ready");
    }

    #[test]
    fn normalize_since_filter_supports_relative_and_absolute_values() {
        let relative = normalize_since_filter("24h").expect("relative filter should parse");
        let absolute =
            normalize_since_filter("2026-03-10T12:00:00Z").expect("rfc3339 filter should parse");

        assert_eq!(relative.len(), 19);
        assert_eq!(absolute, "2026-03-10 12:00:00");
    }
}
