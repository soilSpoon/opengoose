use crate::error::PersistenceError;
use crate::models::{AlertHistoryRow, AlertRuleRow};

const DEFAULT_ALERT_HISTORY_LIMIT: i64 = 50;

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

/// Query parameters for paginated alert history.
#[derive(Debug, Clone)]
pub struct AlertHistoryQuery {
    pub limit: i64,
    pub offset: i64,
    pub rule: Option<String>,
    pub since: Option<String>,
}

impl Default for AlertHistoryQuery {
    fn default() -> Self {
        Self {
            limit: DEFAULT_ALERT_HISTORY_LIMIT,
            offset: 0,
            rule: None,
            since: None,
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

/// Internal helper for fetching all three system-metric counts in one SQL statement.
#[derive(diesel::QueryableByName)]
pub(crate) struct MetricsRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) queue_backlog: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) failed_runs: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) error_rate: i64,
}
