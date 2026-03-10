use std::sync::Arc;

use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Database;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{AlertHistoryRow, AlertRuleRow, NewAlertHistory, NewAlertRule};
use crate::schema::{alert_history, alert_rules};

/// Notification action to execute when an alert fires.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertAction {
    /// POST a JSON payload to the given URL.
    Webhook { url: String },
    /// Emit a tracing::warn! log entry.
    Log,
    /// Queue a message to a specific channel via the event bus.
    ChannelMessage {
        platform: String,
        channel_id: String,
    },
}

impl AlertAction {
    /// Serialize a slice of actions to a JSON string for DB storage.
    pub fn serialize_list(actions: &[AlertAction]) -> String {
        serde_json::to_string(actions).unwrap_or_else(|_| "[]".to_string())
    }

    /// Deserialize a JSON string from the DB into a Vec of actions.
    pub fn deserialize_list(s: &str) -> Vec<AlertAction> {
        serde_json::from_str(s).unwrap_or_default()
    }
}

/// System health metric that an alert rule monitors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertMetric {
    /// Number of pending/failed messages in the message queue.
    QueueBacklog,
    /// Number of orchestration runs with status 'failed'.
    FailedRuns,
    /// Number of orchestration runs with status 'error'.
    ErrorRate,
}

impl AlertMetric {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QueueBacklog => "queue_backlog",
            Self::FailedRuns => "failed_runs",
            Self::ErrorRate => "error_rate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queue_backlog" => Some(Self::QueueBacklog),
            "failed_runs" => Some(Self::FailedRuns),
            "error_rate" => Some(Self::ErrorRate),
            _ => None,
        }
    }

    /// All valid metric names, for help text.
    pub fn variants() -> &'static [&'static str] {
        &["queue_backlog", "failed_runs", "error_rate"]
    }
}

impl std::fmt::Display for AlertMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Comparison operator for threshold evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertCondition {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl AlertCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GreaterThan => "gt",
            Self::LessThan => "lt",
            Self::GreaterThanOrEqual => "gte",
            Self::LessThanOrEqual => "lte",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "gt" => Some(Self::GreaterThan),
            "lt" => Some(Self::LessThan),
            "gte" => Some(Self::GreaterThanOrEqual),
            "lte" => Some(Self::LessThanOrEqual),
            _ => None,
        }
    }

    pub fn variants() -> &'static [&'static str] {
        &["gt", "lt", "gte", "lte"]
    }

    /// Evaluate `value <op> threshold`.
    pub fn evaluate(&self, value: f64, threshold: f64) -> bool {
        match self {
            Self::GreaterThan => value > threshold,
            Self::LessThan => value < threshold,
            Self::GreaterThanOrEqual => value >= threshold,
            Self::LessThanOrEqual => value <= threshold,
        }
    }
}

impl std::fmt::Display for AlertCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed alert rule ready for use.
#[derive(Debug, Clone)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metric: AlertMetric,
    pub condition: AlertCondition,
    pub threshold: f64,
    pub enabled: bool,
    pub actions: Vec<AlertAction>,
    pub created_at: String,
    pub updated_at: String,
}

impl TryFrom<AlertRuleRow> for AlertRule {
    type Error = PersistenceError;

    fn try_from(row: AlertRuleRow) -> Result<Self, Self::Error> {
        let metric = AlertMetric::parse(&row.metric).ok_or_else(|| {
            PersistenceError::InvalidEnumValue(format!("unknown AlertMetric: {}", row.metric))
        })?;
        let condition = AlertCondition::parse(&row.condition).ok_or_else(|| {
            PersistenceError::InvalidEnumValue(format!("unknown AlertCondition: {}", row.condition))
        })?;
        Ok(AlertRule {
            id: row.id,
            name: row.name,
            description: row.description,
            metric,
            condition,
            threshold: row.threshold,
            enabled: row.enabled != 0,
            actions: AlertAction::deserialize_list(&row.actions),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// A record of an alert that was fired.
#[derive(Debug, Clone)]
pub struct AlertHistoryEntry {
    pub id: i32,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub triggered_at: String,
}

impl From<AlertHistoryRow> for AlertHistoryEntry {
    fn from(row: AlertHistoryRow) -> Self {
        AlertHistoryEntry {
            id: row.id,
            rule_id: row.rule_id,
            rule_name: row.rule_name,
            metric: row.metric,
            value: row.value,
            triggered_at: row.triggered_at,
        }
    }
}

/// Snapshot of system health metrics for evaluating alert rules.
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub queue_backlog: f64,
    pub failed_runs: f64,
    pub error_rate: f64,
}

/// Internal helper for raw SQL COUNT(*) queries.
#[derive(diesel::QueryableByName)]
struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    count: i64,
}

/// Store for managing alert rules and alert history.
pub struct AlertStore {
    db: Arc<Database>,
}

impl AlertStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new alert rule.
    pub fn create(
        &self,
        name: &str,
        description: Option<&str>,
        metric: &AlertMetric,
        condition: &AlertCondition,
        threshold: f64,
        actions: &[AlertAction],
    ) -> PersistenceResult<AlertRule> {
        let id = Uuid::new_v4().to_string();
        let actions_json = AlertAction::serialize_list(actions);
        let new_rule = NewAlertRule {
            id: &id,
            name,
            description,
            metric: metric.as_str(),
            condition: condition.as_str(),
            threshold,
            actions: &actions_json,
        };

        self.db.with(|conn| {
            let row = diesel::insert_into(alert_rules::table)
                .values(&new_rule)
                .returning(AlertRuleRow::as_returning())
                .get_result(conn)?;
            AlertRule::try_from(row)
        })
    }

    /// List all alert rules.
    pub fn list(&self) -> PersistenceResult<Vec<AlertRule>> {
        let rows = self.db.with(|conn| {
            Ok(alert_rules::table
                .order(alert_rules::created_at.asc())
                .select(AlertRuleRow::as_select())
                .load(conn)?)
        })?;

        rows.into_iter()
            .map(AlertRule::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Get a rule by name.
    pub fn get_by_name(&self, name: &str) -> PersistenceResult<Option<AlertRule>> {
        let row = self.db.with(|conn| {
            Ok(alert_rules::table
                .filter(alert_rules::name.eq(name))
                .select(AlertRuleRow::as_select())
                .first(conn)
                .optional()?)
        })?;

        row.map(AlertRule::try_from).transpose()
    }

    /// Enable or disable a rule by name. Returns true if the rule was found.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> PersistenceResult<bool> {
        let count = self.db.with(|conn| {
            Ok(
                diesel::update(alert_rules::table.filter(alert_rules::name.eq(name)))
                    .set(alert_rules::enabled.eq(if enabled { 1 } else { 0 }))
                    .execute(conn)?,
            )
        })?;
        Ok(count > 0)
    }

    /// Delete a rule by name. Returns true if a rule was deleted.
    pub fn delete(&self, name: &str) -> PersistenceResult<bool> {
        let count = self.db.with(|conn| {
            Ok(
                diesel::delete(alert_rules::table.filter(alert_rules::name.eq(name)))
                    .execute(conn)?,
            )
        })?;
        Ok(count > 0)
    }

    /// Record that a rule was triggered.
    pub fn record_trigger(
        &self,
        rule: &AlertRule,
        value: f64,
    ) -> PersistenceResult<AlertHistoryEntry> {
        self.db.with(|conn| {
            let row = diesel::insert_into(alert_history::table)
                .values(NewAlertHistory {
                    rule_id: &rule.id,
                    rule_name: &rule.name,
                    metric: rule.metric.as_str(),
                    value,
                })
                .returning(AlertHistoryRow::as_returning())
                .get_result(conn)?;
            Ok(AlertHistoryEntry::from(row))
        })
    }

    /// Collect a snapshot of current system health metrics.
    pub fn current_metrics(&self) -> PersistenceResult<SystemMetrics> {
        self.db.with(|conn| {
            let queue_backlog: i64 = diesel::sql_query(
                "SELECT COUNT(*) AS count FROM message_queue \
                 WHERE status IN ('pending', 'failed')",
            )
            .get_result::<CountRow>(conn)
            .map(|r| r.count)
            .unwrap_or(0);

            let failed_runs: i64 = diesel::sql_query(
                "SELECT COUNT(*) AS count FROM orchestration_runs WHERE status = 'failed'",
            )
            .get_result::<CountRow>(conn)
            .map(|r| r.count)
            .unwrap_or(0);

            let error_rate: i64 = diesel::sql_query(
                "SELECT COUNT(*) AS count FROM orchestration_runs WHERE status = 'error'",
            )
            .get_result::<CountRow>(conn)
            .map(|r| r.count)
            .unwrap_or(0);

            Ok(SystemMetrics {
                queue_backlog: queue_backlog as f64,
                failed_runs: failed_runs as f64,
                error_rate: error_rate as f64,
            })
        })
    }

    /// Get recent alert history, newest first.
    pub fn history(&self, limit: i64) -> PersistenceResult<Vec<AlertHistoryEntry>> {
        let rows = self.db.with(|conn| {
            Ok(alert_history::table
                .order((alert_history::triggered_at.desc(), alert_history::id.desc()))
                .limit(limit)
                .select(AlertHistoryRow::as_select())
                .load(conn)?)
        })?;

        Ok(rows.into_iter().map(AlertHistoryEntry::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn make_store() -> AlertStore {
        let db = Arc::new(Database::open_in_memory().unwrap());
        AlertStore::new(db)
    }

    #[test]
    fn test_create_and_list() {
        let store = make_store();
        store
            .create(
                "high-queue",
                Some("Queue too large"),
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                100.0,
                &[],
            )
            .unwrap();

        let rules = store.list().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "high-queue");
        assert_eq!(rules[0].threshold, 100.0);
        assert!(rules[0].enabled);
    }

    #[test]
    fn test_delete_rule() {
        let store = make_store();
        store
            .create(
                "temp-rule",
                None,
                &AlertMetric::FailedRuns,
                &AlertCondition::GreaterThanOrEqual,
                5.0,
                &[],
            )
            .unwrap();

        assert!(store.delete("temp-rule").unwrap());
        assert!(!store.delete("temp-rule").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_get_by_name() {
        let store = make_store();
        store
            .create(
                "my-rule",
                None,
                &AlertMetric::ErrorRate,
                &AlertCondition::LessThan,
                10.0,
                &[],
            )
            .unwrap();

        let rule = store.get_by_name("my-rule").unwrap();
        assert!(rule.is_some());
        assert!(store.get_by_name("missing").unwrap().is_none());
    }

    #[test]
    fn test_set_enabled() {
        let store = make_store();
        store
            .create(
                "toggle-rule",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                50.0,
                &[],
            )
            .unwrap();

        assert!(store.set_enabled("toggle-rule", false).unwrap());
        let rule = store.get_by_name("toggle-rule").unwrap().unwrap();
        assert!(!rule.enabled);

        assert!(store.set_enabled("toggle-rule", true).unwrap());
        let rule = store.get_by_name("toggle-rule").unwrap().unwrap();
        assert!(rule.enabled);
    }

    #[test]
    fn test_record_trigger_and_history() {
        let store = make_store();
        let rule = store
            .create(
                "fired-rule",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                10.0,
                &[],
            )
            .unwrap();

        store.record_trigger(&rule, 42.0).unwrap();
        store.record_trigger(&rule, 55.0).unwrap();

        let history = store.history(10).unwrap();
        assert_eq!(history.len(), 2);
        // Newest first
        assert_eq!(history[0].value, 55.0);
    }

    #[test]
    fn test_condition_evaluate() {
        assert!(AlertCondition::GreaterThan.evaluate(10.0, 5.0));
        assert!(!AlertCondition::GreaterThan.evaluate(5.0, 10.0));
        assert!(AlertCondition::LessThan.evaluate(3.0, 10.0));
        assert!(AlertCondition::GreaterThanOrEqual.evaluate(5.0, 5.0));
        assert!(AlertCondition::LessThanOrEqual.evaluate(5.0, 5.0));
    }

    #[test]
    fn test_metric_roundtrip() {
        for m in [
            AlertMetric::QueueBacklog,
            AlertMetric::FailedRuns,
            AlertMetric::ErrorRate,
        ] {
            assert_eq!(AlertMetric::parse(m.as_str()), Some(m));
        }
        assert_eq!(AlertMetric::parse("bogus"), None);
    }

    #[test]
    fn test_condition_roundtrip() {
        for c in [
            AlertCondition::GreaterThan,
            AlertCondition::LessThan,
            AlertCondition::GreaterThanOrEqual,
            AlertCondition::LessThanOrEqual,
        ] {
            assert_eq!(AlertCondition::parse(c.as_str()), Some(c));
        }
        assert_eq!(AlertCondition::parse("bogus"), None);
    }

    #[test]
    fn test_history_empty() {
        let store = make_store();
        let history = store.history(10).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_current_metrics_empty_db() {
        let store = make_store();
        let metrics = store.current_metrics().unwrap();
        assert_eq!(metrics.queue_backlog, 0.0);
        assert_eq!(metrics.failed_runs, 0.0);
        assert_eq!(metrics.error_rate, 0.0);
    }

    #[test]
    fn test_current_metrics_reflects_queue_and_runs() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = AlertStore::new(db.clone());

        // Ensure session exists (message_queue has FK on session_key)
        db.with(|conn| {
            diesel::sql_query("INSERT INTO sessions (session_key) VALUES ('sess1')")
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        // Insert a pending message into the queue
        let mq = crate::MessageQueue::new(db.clone());
        mq.enqueue("sess1", "run1", "agent-a", "agent-b", "payload", crate::MessageType::Task)
            .unwrap();

        // Insert a failed orchestration run
        let orch = crate::OrchestrationStore::new(db.clone());
        orch.create_run("run1", "sess1", "team1", "chain", "input", 2)
            .unwrap();
        orch.fail_run("run1", "error msg").unwrap();

        // Insert an orchestration run with 'error' status via raw SQL
        // (no API method sets this status, but current_metrics queries for it)
        db.with(|conn| {
            diesel::sql_query(
                "INSERT INTO sessions (session_key) VALUES ('sess2') ON CONFLICT DO NOTHING",
            )
            .execute(conn)?;
            diesel::sql_query(
                "INSERT INTO orchestration_runs \
                 (team_run_id, session_key, team_name, workflow, input, status, total_steps) \
                 VALUES ('run2', 'sess2', 'team2', 'chain', 'input2', 'error', 1)",
            )
            .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let metrics = store.current_metrics().unwrap();
        assert_eq!(metrics.queue_backlog, 1.0);
        assert_eq!(metrics.failed_runs, 1.0);
        assert_eq!(metrics.error_rate, 1.0);
    }

    #[test]
    fn test_alert_metric_display() {
        assert_eq!(format!("{}", AlertMetric::QueueBacklog), "queue_backlog");
        assert_eq!(format!("{}", AlertMetric::FailedRuns), "failed_runs");
        assert_eq!(format!("{}", AlertMetric::ErrorRate), "error_rate");
    }

    #[test]
    fn test_alert_condition_display() {
        assert_eq!(format!("{}", AlertCondition::GreaterThan), "gt");
        assert_eq!(format!("{}", AlertCondition::LessThan), "lt");
        assert_eq!(format!("{}", AlertCondition::GreaterThanOrEqual), "gte");
        assert_eq!(format!("{}", AlertCondition::LessThanOrEqual), "lte");
    }

    #[test]
    fn test_alert_metric_variants() {
        let v = AlertMetric::variants();
        assert_eq!(v.len(), 3);
        assert!(v.contains(&"queue_backlog"));
    }

    #[test]
    fn test_alert_condition_variants() {
        let v = AlertCondition::variants();
        assert_eq!(v.len(), 4);
        assert!(v.contains(&"gt"));
    }

    #[test]
    fn test_set_enabled_nonexistent() {
        let store = make_store();
        let result = store.set_enabled("does-not-exist", false).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_alert_action_serialization_roundtrip() {
        let actions = vec![
            AlertAction::Log,
            AlertAction::Webhook {
                url: "https://example.com/hook".into(),
            },
            AlertAction::ChannelMessage {
                platform: "slack".into(),
                channel_id: "C123".into(),
            },
        ];
        let json = AlertAction::serialize_list(&actions);
        let roundtrip = AlertAction::deserialize_list(&json);
        assert_eq!(actions, roundtrip);
    }

    #[test]
    fn test_alert_action_empty_list() {
        let json = AlertAction::serialize_list(&[]);
        assert_eq!(json, "[]");
        let result = AlertAction::deserialize_list(&json);
        assert!(result.is_empty());
    }

    #[test]
    fn test_create_with_actions() {
        let store = make_store();
        let actions = vec![
            AlertAction::Log,
            AlertAction::Webhook {
                url: "https://hooks.example.com/alert".into(),
            },
        ];
        let rule = store
            .create(
                "action-rule",
                Some("Rule with actions"),
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                10.0,
                &actions,
            )
            .unwrap();

        assert_eq!(rule.actions.len(), 2);
        assert!(matches!(rule.actions[0], AlertAction::Log));
        assert!(
            matches!(&rule.actions[1], AlertAction::Webhook { url } if url == "https://hooks.example.com/alert")
        );

        // Verify persistence via list
        let rules = store.list().unwrap();
        assert_eq!(rules[0].actions.len(), 2);
    }
}
