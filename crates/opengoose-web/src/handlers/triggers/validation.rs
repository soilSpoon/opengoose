use opengoose_persistence::Trigger;

use crate::handlers::AppError;

use super::requests::{CreateTriggerRequest, TestTriggerRequest, UpdateTriggerRequest};

pub(super) struct ValidatedCreateTrigger {
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
}

pub(super) struct ValidatedUpdateTrigger {
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
}

pub(super) fn validate_create_request(
    body: CreateTriggerRequest,
) -> Result<ValidatedCreateTrigger, AppError> {
    Ok(ValidatedCreateTrigger {
        name: required_trimmed(body.name, "name")?,
        trigger_type: required_trimmed(body.trigger_type, "trigger_type")?,
        condition_json: validated_condition_json(body.condition_json)?,
        team_name: required_trimmed(body.team_name, "team_name")?,
        input: body.input.unwrap_or_default(),
    })
}

pub(super) fn validate_update_request(
    body: UpdateTriggerRequest,
) -> Result<ValidatedUpdateTrigger, AppError> {
    Ok(ValidatedUpdateTrigger {
        trigger_type: required_trimmed(body.trigger_type, "trigger_type")?,
        condition_json: validated_condition_json(body.condition_json)?,
        team_name: required_trimmed(body.team_name, "team_name")?,
        input: body.input.unwrap_or_default(),
    })
}

pub(super) fn resolve_test_input(
    trigger: &Trigger,
    body: Option<TestTriggerRequest>,
    name: &str,
) -> String {
    body.and_then(|payload| payload.input)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if trigger.input.is_empty() {
                format!("Test run fired from the web dashboard for trigger {name}")
            } else {
                trigger.input.clone()
            }
        })
}

pub(super) fn trigger_not_found(name: &str) -> AppError {
    AppError::NotFound(format!("trigger `{name}` not found"))
}

fn required_trimmed(value: String, field_name: &str) -> Result<String, AppError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::UnprocessableEntity(format!(
            "`{field_name}` must not be empty"
        )));
    }

    Ok(trimmed)
}

fn validated_condition_json(condition_json: Option<String>) -> Result<String, AppError> {
    let condition_json = condition_json.unwrap_or_else(|| "{}".into());
    serde_json::from_str::<serde_json::Value>(&condition_json).map_err(|error| {
        AppError::BadRequest(format!("`condition_json` is not valid JSON: {error}"))
    })?;

    Ok(condition_json)
}
