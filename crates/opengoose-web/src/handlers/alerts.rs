use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use opengoose_persistence::{AlertCondition, AlertHistoryEntry, AlertMetric, AlertRule};

use super::AppError;
use crate::state::AppState;

// ── Response types ────────────────────────────────────────────────────────────

/// JSON response representing a persisted alert rule.
#[derive(Serialize)]
pub struct AlertRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metric: String,
    pub condition: String,
    pub threshold: f64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<AlertRule> for AlertRuleResponse {
    fn from(r: AlertRule) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            metric: r.metric.to_string(),
            condition: r.condition.to_string(),
            threshold: r.threshold,
            enabled: r.enabled,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// JSON response representing a single alert trigger event.
#[derive(Serialize)]
pub struct AlertHistoryResponse {
    pub id: i32,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub triggered_at: String,
}

impl From<AlertHistoryEntry> for AlertHistoryResponse {
    fn from(e: AlertHistoryEntry) -> Self {
        Self {
            id: e.id,
            rule_id: e.rule_id,
            rule_name: e.rule_name,
            metric: e.metric,
            value: e.value,
            triggered_at: e.triggered_at,
        }
    }
}

// ── Request types ─────────────────────────────────────────────────────────────

/// JSON request body for creating a new alert rule.
#[derive(Deserialize)]
pub struct CreateAlertRequest {
    pub name: String,
    pub description: Option<String>,
    /// One of: queue_backlog, failed_runs, error_rate
    pub metric: String,
    /// One of: gt, lt, gte, lte
    pub condition: String,
    pub threshold: f64,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/alerts
pub async fn list_alerts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AlertRuleResponse>>, AppError> {
    let rules = state.alert_store.list()?;
    Ok(Json(
        rules.into_iter().map(AlertRuleResponse::from).collect(),
    ))
}

/// POST /api/alerts
pub async fn create_alert(
    State(state): State<AppState>,
    Json(body): Json<CreateAlertRequest>,
) -> Result<Json<AlertRuleResponse>, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`name` must not be empty".into(),
        ));
    }
    if !body.threshold.is_finite() {
        return Err(AppError::UnprocessableEntity(
            "`threshold` must be a finite number".into(),
        ));
    }
    let metric = AlertMetric::parse(&body.metric).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unknown metric `{}`. Valid: {}",
            body.metric,
            AlertMetric::variants().join(", ")
        ))
    })?;
    let condition = AlertCondition::parse(&body.condition).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unknown condition `{}`. Valid: {}",
            body.condition,
            AlertCondition::variants().join(", ")
        ))
    })?;

    let rule = state.alert_store.create(
        &body.name,
        body.description.as_deref(),
        &metric,
        &condition,
        body.threshold,
    )?;

    Ok(Json(AlertRuleResponse::from(rule)))
}

/// DELETE /api/alerts/:name
pub async fn delete_alert(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.alert_store.delete(&name)? {
        Ok(Json(serde_json::json!({ "deleted": name })))
    } else {
        Err(AppError::NotFound(format!("alert rule `{name}` not found")))
    }
}

/// GET /api/alerts/history
pub async fn alert_history(
    State(state): State<AppState>,
) -> Result<Json<Vec<AlertHistoryResponse>>, AppError> {
    let entries = state.alert_store.history(50)?;
    Ok(Json(
        entries
            .into_iter()
            .map(AlertHistoryResponse::from)
            .collect(),
    ))
}

/// POST /api/alerts/test
///
/// Evaluates all enabled rules against current system metrics and records any
/// triggered alerts. Returns the list of triggered rule names.
pub async fn test_alerts(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rules = state.alert_store.list()?;
    let metrics = state.alert_store.current_metrics()?;

    let mut triggered: Vec<String> = Vec::new();

    for rule in rules.iter().filter(|r| r.enabled) {
        let value = match rule.metric {
            AlertMetric::QueueBacklog => metrics.queue_backlog,
            AlertMetric::FailedRuns => metrics.failed_runs,
            AlertMetric::ErrorRate => metrics.error_rate,
        };

        if rule.condition.evaluate(value, rule.threshold) {
            state.alert_store.record_trigger(rule, value)?;
            triggered.push(rule.name.clone());
        }
    }

    Ok(Json(serde_json::json!({
        "metrics": {
            "queue_backlog": metrics.queue_backlog,
            "failed_runs": metrics.failed_runs,
            "error_rate": metrics.error_rate,
        },
        "triggered": triggered,
    })))
}
