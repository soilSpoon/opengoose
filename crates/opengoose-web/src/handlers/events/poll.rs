use axum::Json;
use axum::extract::{Query, State};
use opengoose_persistence::{EventHistoryQuery, EventStore, normalize_since_filter};
use serde::{Deserialize, Serialize};

use crate::handlers::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventHistoryQueryParams {
    #[serde(default = "default_history_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub gateway: Option<String>,
    pub kind: Option<String>,
    pub session_key: Option<String>,
    pub since: Option<String>,
}

fn default_history_limit() -> i64 {
    100
}

#[derive(Debug, Serialize)]
pub struct EventHistoryResponse {
    pub id: i32,
    pub event_kind: String,
    pub timestamp: String,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EventHistoryPageResponse {
    pub items: Vec<EventHistoryResponse>,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

fn validate_history_query(query: &EventHistoryQueryParams) -> Result<(), AppError> {
    if query.limit <= 0 || query.limit > 1000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 1000, got {}",
            query.limit
        )));
    }
    if query.offset < 0 {
        return Err(AppError::UnprocessableEntity(format!(
            "`offset` must be 0 or greater, got {}",
            query.offset
        )));
    }

    Ok(())
}

/// GET /api/events/history — list persisted event history with filters.
pub async fn list_event_history(
    State(state): State<AppState>,
    Query(query): Query<EventHistoryQueryParams>,
) -> Result<Json<EventHistoryPageResponse>, AppError> {
    validate_history_query(&query)?;

    let store = EventStore::new(state.db.clone());
    let mut entries = store.list(&EventHistoryQuery {
        limit: query.limit + 1,
        offset: query.offset,
        event_kind: query.kind.clone(),
        source_gateway: query.gateway.clone(),
        session_key: query.session_key.clone(),
        since: query
            .since
            .as_deref()
            .map(normalize_since_filter)
            .transpose()
            .map_err(AppError::UnprocessableEntity)?,
    })?;

    let has_more = entries.len() as i64 > query.limit;
    if has_more {
        entries.truncate(query.limit as usize);
    }

    Ok(Json(EventHistoryPageResponse {
        items: entries
            .into_iter()
            .map(|entry| EventHistoryResponse {
                id: entry.id,
                event_kind: entry.event_kind,
                timestamp: entry.timestamp,
                source_gateway: entry.source_gateway,
                session_key: entry.session_key,
                payload: entry.payload,
            })
            .collect(),
        limit: query.limit,
        offset: query.offset,
        has_more,
    }))
}
