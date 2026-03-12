use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteConnection};

use crate::error::PersistenceResult;
use crate::models::EventHistoryRow;
use crate::schema::event_history;

use super::{EventHistoryEntry, EventHistoryQuery};

type EventHistoryStatement<'a> = event_history::BoxedQuery<'a, Sqlite>;

pub(super) fn load_event_history(
    conn: &mut SqliteConnection,
    query: &EventHistoryQuery,
) -> PersistenceResult<Vec<EventHistoryEntry>> {
    let statement = apply_query_filters(event_history::table.into_boxed::<Sqlite>(), query);

    let rows = statement
        .order((event_history::timestamp.desc(), event_history::id.desc()))
        .offset(query.offset)
        .limit(query.limit)
        .select(EventHistoryRow::as_select())
        .load(conn)?;

    rows.into_iter()
        .map(EventHistoryEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
}

pub(super) fn cleanup_expired_events(
    conn: &mut SqliteConnection,
    retention_days: u32,
) -> PersistenceResult<usize> {
    let cutoff = format!("-{retention_days} days");

    let deleted =
        diesel::sql_query("DELETE FROM event_history WHERE timestamp < datetime('now', ?1)")
            .bind::<Text, _>(&cutoff)
            .execute(conn)?;

    Ok(deleted)
}

fn apply_query_filters<'a>(
    mut statement: EventHistoryStatement<'a>,
    query: &'a EventHistoryQuery,
) -> EventHistoryStatement<'a> {
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

    statement
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
