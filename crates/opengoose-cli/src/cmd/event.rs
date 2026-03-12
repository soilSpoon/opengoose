use std::sync::Arc;

use crate::error::{CliError, CliResult};
use clap::Subcommand;
use serde_json::json;

use opengoose_persistence::{normalize_since_filter, Database, EventHistoryQuery, EventStore};

use crate::cmd::output::{format_table, CliOutput};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose event history --limit 100\n  opengoose event history --filter gateway:discord --since 24h\n  opengoose --json event history --filter kind:message_received"
)]
pub enum EventAction {
    /// Show persisted event history
    History {
        /// Maximum number of entries to return (default 100, max 1000)
        #[arg(long, default_value = "100")]
        limit: i64,
        /// Pagination offset
        #[arg(long, default_value = "0")]
        offset: i64,
        /// Filter in key:value form. Supported keys: gateway, session, kind
        #[arg(long)]
        filter: Vec<String>,
        /// Only include events at or after this cutoff (for example `24h` or RFC3339)
        #[arg(long)]
        since: Option<String>,
    },
}

pub fn execute(action: EventAction, output: CliOutput) -> CliResult<()> {
    match action {
        EventAction::History {
            limit,
            offset,
            filter,
            since,
        } => cmd_history(limit, offset, &filter, since.as_deref(), output),
    }
}

fn cmd_history(
    limit: i64,
    offset: i64,
    filters: &[String],
    since: Option<&str>,
    output: CliOutput,
) -> CliResult<()> {
    if limit <= 0 || limit > 1000 {
        return Err(CliError::Validation(format!(
            "`--limit` must be between 1 and 1000, got {limit}"
        )));
    }
    if offset < 0 {
        return Err(CliError::Validation(format!(
            "`--offset` must be 0 or greater, got {offset}"
        )));
    }

    let mut query = EventHistoryQuery {
        limit: limit + 1,
        offset,
        ..EventHistoryQuery::default()
    };

    for filter in filters {
        let (key, value) = filter.split_once(':').ok_or_else(|| {
            CliError::Validation(format!("invalid filter `{filter}`; expected key:value"))
        })?;
        if value.trim().is_empty() {
            return Err(CliError::Validation(format!(
                "invalid filter `{filter}`; value must not be empty"
            )));
        }

        match key.trim() {
            "gateway" | "source_gateway" => query.source_gateway = Some(value.trim().to_string()),
            "session" | "session_key" => query.session_key = Some(value.trim().to_string()),
            "kind" | "event_kind" => query.event_kind = Some(value.trim().to_string()),
            other => {
                return Err(CliError::Validation(format!(
                    "unsupported filter key `{other}`; supported keys: gateway, session, kind"
                )))
            }
        }
    }

    if let Some(value) = since {
        query.since = Some(
            normalize_since_filter(value).map_err(|err| CliError::Validation(err.to_string()))?,
        );
    }

    let store = EventStore::new(Arc::new(Database::open()?));
    let mut entries = store.list(&query)?;
    let has_more = entries.len() as i64 > limit;
    if has_more {
        entries.truncate(limit as usize);
    }

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "event.history",
            "limit": limit,
            "offset": offset,
            "has_more": has_more,
            "items": entries.into_iter().map(|entry| json!({
                "id": entry.id,
                "event_kind": entry.event_kind,
                "timestamp": entry.timestamp,
                "source_gateway": entry.source_gateway,
                "session_key": entry.session_key,
                "payload": entry.payload,
            })).collect::<Vec<_>>(),
        }))?;
        return Ok(());
    }

    if entries.is_empty() {
        println!("No event history found.");
        return Ok(());
    }

    let rows = entries
        .iter()
        .map(|entry| {
            vec![
                entry.timestamp.clone(),
                entry.event_kind.clone(),
                entry
                    .source_gateway
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                entry.session_key.clone().unwrap_or_else(|| "-".to_string()),
                compact_payload(&entry.payload),
            ]
        })
        .collect::<Vec<_>>();

    print!(
        "{}",
        format_table(
            &["TIMESTAMP", "KIND", "GATEWAY", "SESSION", "PAYLOAD"],
            &rows
        )
    );
    if has_more {
        println!(
            "Showing {limit} event(s) starting at offset {offset}. Pass --offset {} to continue.",
            offset + limit
        );
    }

    Ok(())
}

fn compact_payload(payload: &serde_json::Value) -> String {
    const MAX_LEN: usize = 80;

    let raw = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    let mut compact = raw.replace('\n', " ");
    if compact.len() <= MAX_LEN {
        return compact;
    }

    compact.truncate(MAX_LEN.saturating_sub(3));
    compact.push_str("...");
    compact
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_payload_truncates_large_values() {
        let payload = json!({
            "type": "message_received",
            "content": "x".repeat(200)
        });

        let compact = compact_payload(&payload);

        assert!(compact.ends_with("..."));
        assert!(compact.len() <= 80);
    }
}
