use axum::Json;
use axum::extract::{Query, State};

use opengoose_persistence::{AlertMetric, AlertRule, SystemMetrics};

use crate::handlers::AppError;
use crate::state::AppState;

use super::requests::TestAlertQueryParams;
use super::responses::test_alerts_json;

/// POST /api/alerts/test
///
/// Evaluates all enabled rules against current system metrics and records any
/// triggered alerts. Returns the list of triggered rule names.
///
/// Accepts optional query params:
///   - `rule`: restrict evaluation to a single rule by name
///   - `dry_run`: evaluate without persisting trigger history
pub async fn test_alerts(
    State(state): State<AppState>,
    Query(params): Query<TestAlertQueryParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rules = state.alert_store.list()?;
    let metrics = state.alert_store.current_metrics()?;

    let mut triggered = Vec::new();
    for rule in rules
        .iter()
        .filter(|rule| rule.enabled && matches_rule_filter(rule, params.rule.as_deref()))
    {
        let value = metric_value(&rule.metric, &metrics);
        if rule.condition.evaluate(value, rule.threshold) {
            if !params.dry_run {
                state.alert_store.record_trigger(rule, value)?;
            }
            triggered.push(rule.name.clone());
        }
    }

    Ok(test_alerts_json(metrics, triggered, params.dry_run))
}

fn matches_rule_filter(rule: &AlertRule, filter: Option<&str>) -> bool {
    match filter {
        Some(name) => rule.name == name,
        None => true,
    }
}

fn metric_value(metric: &AlertMetric, metrics: &SystemMetrics) -> f64 {
    match metric {
        AlertMetric::QueueBacklog => metrics.queue_backlog,
        AlertMetric::FailedRuns => metrics.failed_runs,
        AlertMetric::ErrorRate => metrics.error_rate,
    }
}
