use std::sync::Arc;

use diesel::prelude::*;
use uuid::Uuid;

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::models::{AlertHistoryRow, AlertRuleRow, NewAlertHistory, NewAlertRule};
use crate::schema::{alert_history, alert_rules};

use super::types::{
    AlertAction, AlertCondition, AlertHistoryEntry, AlertHistoryQuery, AlertMetric, AlertRule,
    MetricsRow, SystemMetrics,
};

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
    ///
    /// Uses a single SQL statement with scalar subqueries to fetch all three
    /// counts in one round-trip instead of three separate queries.
    pub fn current_metrics(&self) -> PersistenceResult<SystemMetrics> {
        self.db.with(|conn| {
            let row = diesel::sql_query(
                "SELECT
                    (SELECT COUNT(*) FROM message_queue
                     WHERE status IN ('pending', 'failed')) AS queue_backlog,
                    (SELECT COUNT(*) FROM orchestration_runs
                     WHERE status = 'failed') AS failed_runs,
                    (SELECT COUNT(*) FROM orchestration_runs
                     WHERE status = 'error') AS error_rate",
            )
            .get_result::<MetricsRow>(conn)?;

            Ok(SystemMetrics {
                queue_backlog: row.queue_backlog as f64,
                failed_runs: row.failed_runs as f64,
                error_rate: row.error_rate as f64,
            })
        })
    }

    /// Get recent alert history, newest first.
    pub fn history(&self, limit: i64) -> PersistenceResult<Vec<AlertHistoryEntry>> {
        self.history_by_query(&AlertHistoryQuery {
            limit,
            ..AlertHistoryQuery::default()
        })
    }

    /// Query alert history using optional filters and pagination.
    pub fn history_by_query(
        &self,
        query: &AlertHistoryQuery,
    ) -> PersistenceResult<Vec<AlertHistoryEntry>> {
        let rows = self.db.with(|conn| {
            let mut statement = alert_history::table.into_boxed::<diesel::sqlite::Sqlite>();

            if let Some(rule_name) = query.rule.as_deref() {
                statement = statement.filter(alert_history::rule_name.eq(rule_name));
            }
            if let Some(since) = query.since.as_deref() {
                statement = statement.filter(alert_history::triggered_at.ge(since));
            }

            Ok(statement
                .order((alert_history::triggered_at.desc(), alert_history::id.desc()))
                .offset(query.offset)
                .limit(query.limit)
                .select(AlertHistoryRow::as_select())
                .load(conn)?)
        })?;

        Ok(rows.into_iter().map(AlertHistoryEntry::from).collect())
    }
}
