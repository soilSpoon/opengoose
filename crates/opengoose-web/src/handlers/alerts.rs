use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use opengoose_persistence::{
    AlertAction, AlertCondition, AlertHistoryEntry, AlertHistoryQuery, AlertMetric, AlertRule,
    normalize_since_filter,
};

use super::AppError;
use crate::state::AppState;

// ── Response types ────────────────────────────────────────────────────────────

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

#[derive(Debug, Deserialize)]
pub struct AlertHistoryQueryParams {
    #[serde(default = "default_alert_history_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub rule: Option<String>,
    pub since: Option<String>,
}

fn default_alert_history_limit() -> i64 {
    50
}

impl Default for AlertHistoryQueryParams {
    fn default() -> Self {
        Self {
            limit: default_alert_history_limit(),
            offset: 0,
            rule: None,
            since: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct TestAlertQueryParams {
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
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
        &[] as &[AlertAction],
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
    Query(query): Query<AlertHistoryQueryParams>,
) -> Result<Json<Vec<AlertHistoryResponse>>, AppError> {
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

    let entries = state
        .alert_store
        .history_by_query(&AlertHistoryQuery {
            limit: query.limit,
            offset: query.offset,
            rule: query.rule,
            since: query
                .since
                .as_deref()
                .map(normalize_since_filter)
                .transpose()
                .map_err(AppError::UnprocessableEntity)?,
        })?;
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
    Query(query): Query<TestAlertQueryParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rules = state.alert_store.list()?;
    let metrics = state.alert_store.current_metrics()?;

    let enabled_rules: Vec<_> = rules.iter().filter(|r| r.enabled).collect();
    let matched_rules: Vec<_> = if let Some(rule_name) = query.rule.as_deref() {
        if !enabled_rules.iter().any(|r| r.name == rule_name) {
            if state.alert_store.get_by_name(rule_name)?.is_none() {
                return Err(AppError::NotFound(format!("alert rule `{rule_name}` not found")));
            }
            return Err(AppError::UnprocessableEntity(format!(
                "alert rule `{rule_name}` is disabled",
            )));
        }

        enabled_rules
            .into_iter()
            .filter(|r| r.name == rule_name)
            .collect()
    } else {
        enabled_rules
    };

    let mut triggered: Vec<String> = Vec::new();

    for rule in matched_rules {
        let value = match rule.metric {
            AlertMetric::QueueBacklog => metrics.queue_backlog,
            AlertMetric::FailedRuns => metrics.failed_runs,
            AlertMetric::ErrorRate => metrics.error_rate,
        };

        if rule.condition.evaluate(value, rule.threshold) {
            if !query.dry_run {
                state.alert_store.record_trigger(rule, value)?;
            }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::Json;
    use axum::extract::{Path, Query, State};
    use opengoose_persistence::{
        AlertCondition, AlertMetric, AlertStore, ApiKeyStore, Database, OrchestrationStore,
        ScheduleStore, SessionStore, TriggerStore,
    };
    use opengoose_profiles::ProfileStore;
    use opengoose_teams::TeamStore;
    use opengoose_types::{ChannelMetricsStore, EventBus};

    use super::{
        AlertHistoryQueryParams, CreateAlertRequest, TestAlertQueryParams, alert_history,
        create_alert, delete_alert, list_alerts, test_alerts,
    };
    use crate::error::WebError;
    use crate::state::AppState;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "opengoose-web-alerts-{label}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp test dir should be created");
        dir
    }

    fn make_state() -> AppState {
        let db = Arc::new(Database::open_in_memory().expect("in-memory db should open"));
        AppState {
            db: db.clone(),
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::with_dir(unique_temp_dir("profiles"))),
            team_store: Arc::new(TeamStore::with_dir(unique_temp_dir("teams"))),
            schedule_store: Arc::new(ScheduleStore::new(db.clone())),
            trigger_store: Arc::new(TriggerStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db.clone())),
            api_key_store: Arc::new(ApiKeyStore::new(db)),
            channel_metrics: ChannelMetricsStore::new(),
            event_bus: EventBus::new(256),
        }
    }

    #[tokio::test]
    async fn create_alert_persists_rule_and_list_alerts_returns_it() {
        let state = make_state();

        let Json(created) = create_alert(
            State(state.clone()),
            Json(CreateAlertRequest {
                name: "queue-backlog".into(),
                description: Some("Backlog exceeded".into()),
                metric: "queue_backlog".into(),
                condition: "gt".into(),
                threshold: 5.0,
            }),
        )
        .await
        .expect("create alert should succeed");

        assert_eq!(created.name, "queue-backlog");
        assert_eq!(created.metric, "queue_backlog");
        assert_eq!(created.condition, "gt");
        assert_eq!(created.threshold, 5.0);
        assert!(created.enabled);

        let Json(rules) = list_alerts(State(state))
            .await
            .expect("list alerts should succeed");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, created.id);
        assert_eq!(rules[0].description.as_deref(), Some("Backlog exceeded"));
    }

    #[tokio::test]
    async fn create_alert_rejects_blank_name() {
        let err = create_alert(
            State(make_state()),
            Json(CreateAlertRequest {
                name: "   ".into(),
                description: None,
                metric: "queue_backlog".into(),
                condition: "gt".into(),
                threshold: 1.0,
            }),
        )
        .await
        .expect_err("blank names should be rejected");

        assert!(
            matches!(err, WebError::UnprocessableEntity(message) if message.contains("`name`"))
        );
    }

    #[tokio::test]
    async fn create_alert_rejects_non_finite_threshold() {
        let err = create_alert(
            State(make_state()),
            Json(CreateAlertRequest {
                name: "bad-threshold".into(),
                description: None,
                metric: "failed_runs".into(),
                condition: "gte".into(),
                threshold: f64::NAN,
            }),
        )
        .await
        .expect_err("non-finite thresholds should be rejected");

        assert!(
            matches!(err, WebError::UnprocessableEntity(message) if message.contains("`threshold`"))
        );
    }

    #[tokio::test]
    async fn delete_alert_returns_deleted_payload_and_missing_rule_errors() {
        let state = make_state();
        state
            .alert_store
            .create(
                "queue-backlog",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                10.0,
                &[],
            )
            .expect("rule should be created");

        let Json(deleted) = delete_alert(State(state.clone()), Path("queue-backlog".into()))
            .await
            .expect("delete should succeed");
        assert_eq!(deleted["deleted"].as_str(), Some("queue-backlog"));

        let err = delete_alert(State(state), Path("queue-backlog".into()))
            .await
            .expect_err("deleting a missing rule should fail");
        assert!(matches!(err, WebError::NotFound(message) if message.contains("queue-backlog")));
    }

    #[tokio::test]
    async fn test_alerts_records_only_enabled_matching_rules_and_exposes_history() {
        let state = make_state();
        state
            .alert_store
            .create(
                "queue-backlog",
                Some("fires on any backlog"),
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                -1.0,
                &[],
            )
            .expect("enabled rule should be created");
        state
            .alert_store
            .create(
                "disabled-rule",
                None,
                &AlertMetric::FailedRuns,
                &AlertCondition::GreaterThan,
                -1.0,
                &[],
            )
            .expect("disabled rule should be created");
        state
            .alert_store
            .set_enabled("disabled-rule", false)
            .expect("rule should be disabled");

        let Json(result) = test_alerts(
            State(state.clone()),
            Query(TestAlertQueryParams::default()),
        )
            .await
            .expect("test alerts should succeed");

        assert_eq!(result["metrics"]["queue_backlog"].as_f64(), Some(0.0));
        assert_eq!(result["metrics"]["failed_runs"].as_f64(), Some(0.0));
        assert_eq!(result["triggered"], serde_json::json!(["queue-backlog"]));

        let Json(history) = alert_history(State(state), Query(AlertHistoryQueryParams::default()))
            .await
            .expect("alert history should succeed");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].rule_name, "queue-backlog");
        assert_eq!(history[0].metric, "queue_backlog");
        assert_eq!(history[0].value, 0.0);
    }

    #[tokio::test]
    async fn alert_history_filters_by_rule_query_param() {
        let state = make_state();
        let rule = state
            .alert_store
            .create(
                "queue-rule",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::LessThan,
                100.0,
                &[],
            )
            .expect("rule should be created");
        let other_rule = state
            .alert_store
            .create(
                "error-rule",
                None,
                &AlertMetric::ErrorRate,
                &AlertCondition::LessThan,
                100.0,
                &[],
            )
            .expect("other rule should be created");

        state
            .alert_store
            .record_trigger(&rule, 1.0)
            .expect("trigger should be recorded");
        state
            .alert_store
            .record_trigger(&other_rule, 2.0)
            .expect("other trigger should be recorded");

        let Json(history) = alert_history(
            State(state),
            Query(AlertHistoryQueryParams {
                rule: Some("queue-rule".into()),
                ..AlertHistoryQueryParams::default()
            }),
        )
        .await
        .expect("filtered alert history should be returned");

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].rule_name, "queue-rule");
        assert_eq!(history[0].value, 1.0);
    }

    #[tokio::test]
    async fn test_alerts_with_rule_filter_and_dry_run_does_not_record_history() {
        let state = make_state();
        state
            .alert_store
            .create(
                "queue-rule",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                -1.0,
                &[],
            )
            .expect("rule should be created");
        state
            .alert_store
            .create(
                "error-rule",
                None,
                &AlertMetric::ErrorRate,
                &AlertCondition::GreaterThan,
                -1.0,
                &[],
            )
            .expect("rule should be created");

        let Json(result) = test_alerts(
            State(state.clone()),
            Query(TestAlertQueryParams {
                rule: Some("queue-rule".into()),
                dry_run: true,
            }),
        )
        .await
        .expect("test alerts should run in dry run mode");

        assert_eq!(result["triggered"], serde_json::json!(["queue-rule"]));

        let Json(history) = alert_history(State(state), Query(AlertHistoryQueryParams::default()))
            .await
            .expect("alert history should be queryable");
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn test_alerts_displays_disabled_or_unknown_rule_errors() {
        let state = make_state();
        state
            .alert_store
            .create(
                "disabled-rule",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                -1.0,
                &[],
            )
            .expect("rule should be created");
        state
            .alert_store
            .set_enabled("disabled-rule", false)
            .expect("rule should be disabled");

        let err = test_alerts(
            State(state.clone()),
            Query(TestAlertQueryParams {
                rule: Some("disabled-rule".into()),
                dry_run: false,
            }),
        )
        .await
        .expect_err("disabled rule should error");
        assert!(
            matches!(err, WebError::UnprocessableEntity(message) if message.contains("disabled"))
        );

        let err = test_alerts(
            State(state),
            Query(TestAlertQueryParams {
                rule: Some("missing-rule".into()),
                dry_run: false,
            }),
        )
        .await
        .expect_err("missing rule should error");
        assert!(matches!(err, WebError::NotFound(message) if message.contains("missing-rule")));
    }
}
