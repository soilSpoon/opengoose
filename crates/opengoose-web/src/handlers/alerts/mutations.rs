use axum::Json;
use axum::extract::{Path, State};

use opengoose_persistence::{AlertAction, AlertCondition, AlertMetric};

use crate::handlers::AppError;
use crate::state::AppState;

use super::requests::CreateAlertRequest;
use super::responses::{AlertRuleResponse, deleted_alert_json};

/// POST /api/alerts
pub async fn create_alert(
    State(state): State<AppState>,
    Json(body): Json<CreateAlertRequest>,
) -> Result<Json<AlertRuleResponse>, AppError> {
    validate_create_request(&body)?;

    let rule = state.alert_store.create(
        &body.name,
        body.description.as_deref(),
        &parse_metric(&body.metric)?,
        &parse_condition(&body.condition)?,
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
        Ok(deleted_alert_json(name))
    } else {
        Err(AppError::NotFound(format!("alert rule `{name}` not found")))
    }
}

fn validate_create_request(body: &CreateAlertRequest) -> Result<(), AppError> {
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
    Ok(())
}

fn parse_metric(metric: &str) -> Result<AlertMetric, AppError> {
    AlertMetric::parse(metric).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unknown metric `{metric}`. Valid: {}",
            AlertMetric::variants().join(", ")
        ))
    })
}

fn parse_condition(condition: &str) -> Result<AlertCondition, AppError> {
    AlertCondition::parse(condition).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unknown condition `{condition}`. Valid: {}",
            AlertCondition::variants().join(", ")
        ))
    })
}
