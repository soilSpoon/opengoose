use axum::Json;
use serde::Serialize;
use serde_json::{Value, json};

use opengoose_persistence::{AlertHistoryEntry, AlertRule, SystemMetrics};

/// JSON response representing a persisted alert rule.
#[derive(Debug, Serialize)]
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
    fn from(rule: AlertRule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            metric: rule.metric.to_string(),
            condition: rule.condition.to_string(),
            threshold: rule.threshold,
            enabled: rule.enabled,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        }
    }
}

/// JSON response representing a single alert trigger event.
#[derive(Debug, Serialize)]
pub struct AlertHistoryResponse {
    pub id: i32,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub triggered_at: String,
}

impl From<AlertHistoryEntry> for AlertHistoryResponse {
    fn from(entry: AlertHistoryEntry) -> Self {
        Self {
            id: entry.id,
            rule_id: entry.rule_id,
            rule_name: entry.rule_name,
            metric: entry.metric,
            value: entry.value,
            triggered_at: entry.triggered_at,
        }
    }
}

pub(super) fn alert_rules_json(rules: Vec<AlertRule>) -> Json<Vec<AlertRuleResponse>> {
    Json(rules.into_iter().map(AlertRuleResponse::from).collect())
}

pub(super) fn alert_history_json(
    entries: Vec<AlertHistoryEntry>,
) -> Json<Vec<AlertHistoryResponse>> {
    Json(
        entries
            .into_iter()
            .map(AlertHistoryResponse::from)
            .collect(),
    )
}

pub(super) fn deleted_alert_json(name: String) -> Json<Value> {
    Json(json!({ "deleted": name }))
}

pub(super) fn test_alerts_json(
    metrics: SystemMetrics,
    triggered: Vec<String>,
    dry_run: bool,
) -> Json<Value> {
    Json(json!({
        "metrics": {
            "queue_backlog": metrics.queue_backlog,
            "failed_runs": metrics.failed_runs,
            "error_rate": metrics.error_rate,
        },
        "triggered": triggered,
        "dry_run": dry_run,
    }))
}
