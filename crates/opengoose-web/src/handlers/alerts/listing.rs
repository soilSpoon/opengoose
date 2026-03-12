use axum::Json;
use axum::extract::{Query, State};

use opengoose_persistence::{AlertHistoryQuery, normalize_since_filter};

use crate::handlers::AppError;
use crate::state::AppState;

use super::requests::AlertHistoryQueryParams;
use super::responses::{
    AlertHistoryResponse, AlertRuleResponse, alert_history_json, alert_rules_json,
};

/// GET /api/alerts
pub async fn list_alerts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AlertRuleResponse>>, AppError> {
    Ok(alert_rules_json(state.alert_store.list()?))
}

/// GET /api/alerts/history
pub async fn alert_history(
    State(state): State<AppState>,
    Query(params): Query<AlertHistoryQueryParams>,
) -> Result<Json<Vec<AlertHistoryResponse>>, AppError> {
    let entries = state
        .alert_store
        .history_by_query(&build_history_query(params)?)?;
    Ok(alert_history_json(entries))
}

fn build_history_query(params: AlertHistoryQueryParams) -> Result<AlertHistoryQuery, AppError> {
    let AlertHistoryQueryParams {
        limit,
        offset,
        rule,
        since,
    } = params;

    let since = since
        .as_deref()
        .map(normalize_since_filter)
        .transpose()
        .map_err(AppError::BadRequest)?;

    Ok(AlertHistoryQuery {
        limit: limit.unwrap_or(50),
        offset: offset.unwrap_or(0),
        rule,
        since,
    })
}
