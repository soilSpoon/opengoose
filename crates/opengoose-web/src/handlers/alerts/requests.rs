use serde::Deserialize;

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

/// Query parameters for `GET /api/alerts/history`.
#[derive(Debug, Default, Deserialize)]
pub struct AlertHistoryQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub rule: Option<String>,
    pub since: Option<String>,
}

/// Query parameters for `POST /api/alerts/test`.
#[derive(Debug, Default, Deserialize)]
pub struct TestAlertQueryParams {
    /// Restrict evaluation to a single rule by name.
    pub rule: Option<String>,
    /// When `true`, evaluate rules but do **not** persist trigger history.
    #[serde(default)]
    pub dry_run: bool,
}
