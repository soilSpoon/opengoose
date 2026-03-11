use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::handlers::AppError;
use crate::state::AppState;

use super::validation::{trigger_not_found, validate_create_request, validate_update_request};
use super::{CreateTriggerRequest, SetEnabledRequest, TriggerResponse, UpdateTriggerRequest};

/// GET /api/triggers — list all triggers.
pub async fn list_triggers(
    State(state): State<AppState>,
) -> Result<Json<Vec<TriggerResponse>>, AppError> {
    let triggers = state.trigger_store.list()?;
    Ok(Json(
        triggers.into_iter().map(TriggerResponse::from).collect(),
    ))
}

/// GET /api/triggers/:name — get a single trigger by name.
pub async fn get_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<TriggerResponse>, AppError> {
    let trigger = state
        .trigger_store
        .get_by_name(&name)?
        .ok_or_else(|| trigger_not_found(&name))?;
    Ok(Json(TriggerResponse::from(trigger)))
}

/// POST /api/triggers — create a new trigger.
pub async fn create_trigger(
    State(state): State<AppState>,
    Json(body): Json<CreateTriggerRequest>,
) -> Result<(StatusCode, Json<TriggerResponse>), AppError> {
    let body = validate_create_request(body)?;
    let trigger = state.trigger_store.create(
        &body.name,
        &body.trigger_type,
        &body.condition_json,
        &body.team_name,
        &body.input,
    )?;

    Ok((StatusCode::CREATED, Json(TriggerResponse::from(trigger))))
}

/// PUT /api/triggers/:name — update mutable fields of a trigger.
pub async fn update_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateTriggerRequest>,
) -> Result<Json<TriggerResponse>, AppError> {
    let body = validate_update_request(body)?;
    let trigger = state
        .trigger_store
        .update(
            &name,
            &body.trigger_type,
            &body.condition_json,
            &body.team_name,
            &body.input,
        )?
        .ok_or_else(|| trigger_not_found(&name))?;

    Ok(Json(TriggerResponse::from(trigger)))
}

/// DELETE /api/triggers/:name — remove a trigger.
pub async fn delete_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.trigger_store.remove(&name)? {
        Ok(Json(serde_json::json!({ "deleted": name })))
    } else {
        Err(trigger_not_found(&name))
    }
}

/// PATCH /api/triggers/:name/enabled — enable or disable a trigger.
pub async fn set_trigger_enabled(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SetEnabledRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.trigger_store.set_enabled(&name, body.enabled)? {
        Ok(Json(
            serde_json::json!({ "name": name, "enabled": body.enabled }),
        ))
    } else {
        Err(trigger_not_found(&name))
    }
}
