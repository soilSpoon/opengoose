use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use opengoose_persistence::Trigger;
use opengoose_teams::triggers::TriggerType;

use super::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTriggerInput {
    pub name: String,
    pub trigger_type: String,
    #[serde(default)]
    pub condition_json: String,
    pub team_name: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToggleTriggerRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriggerItem {
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub workflow_title: String,
    pub input: String,
    pub enabled: bool,
    pub last_fired_at: Option<String>,
    pub fire_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// GET /api/triggers — list all workflow triggers.
pub async fn list_triggers(
    State(state): State<AppState>,
) -> Result<Json<Vec<TriggerItem>>, AppError> {
    Ok(Json(list_trigger_items(&state)?))
}

/// POST /api/triggers — create a new workflow trigger.
pub async fn create_trigger(
    State(state): State<AppState>,
    Json(payload): Json<CreateTriggerInput>,
) -> Result<(StatusCode, Json<TriggerItem>), AppError> {
    let trigger = create_trigger_record(&state, payload)?;
    Ok((StatusCode::CREATED, Json(trigger)))
}

/// POST /api/triggers/:name/toggle — enable or disable a trigger.
pub async fn toggle_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<ToggleTriggerRequest>,
) -> Result<Json<TriggerItem>, AppError> {
    Ok(Json(set_trigger_enabled_record(
        &state,
        &name,
        payload.enabled,
    )?))
}

/// DELETE /api/triggers/:name — remove a trigger.
pub async fn delete_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, AppError> {
    delete_trigger_record(&state, &name)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn list_trigger_items(state: &AppState) -> Result<Vec<TriggerItem>, AppError> {
    state
        .trigger_store
        .list()?
        .into_iter()
        .map(|trigger| trigger_item(state, trigger))
        .collect()
}

pub(crate) fn create_trigger_record(
    state: &AppState,
    payload: CreateTriggerInput,
) -> Result<TriggerItem, AppError> {
    let name = required_field(payload.name, "name")?;
    let trigger_type = normalize_trigger_type(&payload.trigger_type)?;
    let team_name = required_field(payload.team_name, "team_name")?;
    let condition_json = normalize_condition_json(&payload.condition_json)?;
    let input = payload.input.trim().to_string();
    let enabled = payload.enabled.unwrap_or(true);

    state.team_store.get(&team_name)?;
    if state.trigger_store.get_by_name(&name)?.is_some() {
        return Err(AppError::BadRequest(format!(
            "trigger {name} already exists"
        )));
    }

    state
        .trigger_store
        .create(&name, &trigger_type, &condition_json, &team_name, &input)?;

    if !enabled {
        state.trigger_store.set_enabled(&name, false)?;
    }

    let saved = state
        .trigger_store
        .get_by_name(&name)?
        .ok_or_else(|| AppError::NotFound(format!("trigger {name}")))?;

    trigger_item(state, saved)
}

pub(crate) fn set_trigger_enabled_record(
    state: &AppState,
    name: &str,
    enabled: bool,
) -> Result<TriggerItem, AppError> {
    let name = required_field(name.to_string(), "name")?;
    if !state.trigger_store.set_enabled(&name, enabled)? {
        return Err(AppError::NotFound(format!("trigger {name}")));
    }

    let trigger = state
        .trigger_store
        .get_by_name(&name)?
        .ok_or_else(|| AppError::NotFound(format!("trigger {name}")))?;

    trigger_item(state, trigger)
}

pub(crate) fn delete_trigger_record(state: &AppState, name: &str) -> Result<(), AppError> {
    let name = required_field(name.to_string(), "name")?;
    if !state.trigger_store.remove(&name)? {
        return Err(AppError::NotFound(format!("trigger {name}")));
    }
    Ok(())
}

fn trigger_item(state: &AppState, trigger: Trigger) -> Result<TriggerItem, AppError> {
    let workflow_title = state
        .team_store
        .get(&trigger.team_name)
        .map(|team| team.title)
        .unwrap_or_else(|_| trigger.team_name.clone());

    Ok(TriggerItem {
        name: trigger.name,
        trigger_type: trigger.trigger_type,
        condition_json: trigger.condition_json,
        team_name: trigger.team_name,
        workflow_title,
        input: trigger.input,
        enabled: trigger.enabled,
        last_fired_at: trigger.last_fired_at,
        fire_count: trigger.fire_count,
        created_at: trigger.created_at,
        updated_at: trigger.updated_at,
    })
}

fn required_field(value: String, field: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!("{field} is required")));
    }
    Ok(trimmed.to_string())
}

fn normalize_trigger_type(raw: &str) -> Result<String, AppError> {
    let normalized = required_field(raw.to_string(), "trigger_type")?;
    if TriggerType::parse(&normalized).is_none() {
        return Err(AppError::BadRequest(format!(
            "unknown trigger_type {normalized}"
        )));
    }
    Ok(normalized)
}

fn normalize_condition_json(raw: &str) -> Result<String, AppError> {
    let candidate = if raw.trim().is_empty() {
        "{}"
    } else {
        raw.trim()
    };
    let value: serde_json::Value = serde_json::from_str(candidate).map_err(|error| {
        AppError::BadRequest(format!("condition_json must be valid JSON: {error}"))
    })?;

    if !value.is_object() {
        return Err(AppError::BadRequest(
            "condition_json must be a JSON object".into(),
        ));
    }

    serde_json::to_string_pretty(&value)
        .map_err(|error| AppError::Internal(format!("failed to format condition_json: {error}")))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::State;

    use super::{
        CreateTriggerInput, create_trigger_record, delete_trigger_record, list_triggers,
        set_trigger_enabled_record,
    };
    use crate::handlers::test_support::{make_state, sample_team};

    #[tokio::test]
    async fn list_triggers_returns_workflow_titles() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("feature-dev", "planner"), false)
            .expect("team should be saved");
        create_trigger_record(
            &state,
            CreateTriggerInput {
                name: "github-pr".into(),
                trigger_type: "webhook_received".into(),
                condition_json: r#"{"path":"/github/pr"}"#.into(),
                team_name: "feature-dev".into(),
                input: "review the PR".into(),
                enabled: None,
            },
        )
        .expect("trigger should be created");

        let Json(items) = list_triggers(State(state))
            .await
            .expect("list_triggers should succeed");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].workflow_title, "feature-dev");
        assert_eq!(items[0].trigger_type, "webhook_received");
    }

    #[test]
    fn create_trigger_record_rejects_unknown_type() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("feature-dev", "planner"), false)
            .expect("team should be saved");

        let err = create_trigger_record(
            &state,
            CreateTriggerInput {
                name: "bad".into(),
                trigger_type: "wat".into(),
                condition_json: "{}".into(),
                team_name: "feature-dev".into(),
                input: String::new(),
                enabled: None,
            },
        )
        .expect_err("unknown trigger type should fail");

        assert!(err.to_string().contains("unknown trigger_type"));
    }

    #[test]
    fn create_trigger_record_rejects_non_object_json() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("feature-dev", "planner"), false)
            .expect("team should be saved");

        let err = create_trigger_record(
            &state,
            CreateTriggerInput {
                name: "bad-json".into(),
                trigger_type: "on_message".into(),
                condition_json: "[]".into(),
                team_name: "feature-dev".into(),
                input: String::new(),
                enabled: None,
            },
        )
        .expect_err("array condition should fail");

        assert!(err.to_string().contains("JSON object"));
    }

    #[test]
    fn create_trigger_record_can_start_disabled() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("feature-dev", "planner"), false)
            .expect("team should be saved");

        let trigger = create_trigger_record(
            &state,
            CreateTriggerInput {
                name: "nightly".into(),
                trigger_type: "schedule_complete".into(),
                condition_json: r#"{"schedule_name":"nightly"}"#.into(),
                team_name: "feature-dev".into(),
                input: String::new(),
                enabled: Some(false),
            },
        )
        .expect("trigger should be created");

        assert!(!trigger.enabled);
    }

    #[test]
    fn set_trigger_enabled_record_updates_state() {
        let state = make_state();
        state
            .team_store
            .save(&sample_team("feature-dev", "planner"), false)
            .expect("team should be saved");
        create_trigger_record(
            &state,
            CreateTriggerInput {
                name: "watch-src".into(),
                trigger_type: "file_watch".into(),
                condition_json: r#"{"pattern":"src/**/*.rs"}"#.into(),
                team_name: "feature-dev".into(),
                input: String::new(),
                enabled: None,
            },
        )
        .expect("trigger should be created");

        let updated =
            set_trigger_enabled_record(&state, "watch-src", false).expect("toggle should succeed");

        assert!(!updated.enabled);
    }

    #[test]
    fn delete_trigger_record_returns_not_found_for_missing_trigger() {
        let state = make_state();

        let err = delete_trigger_record(&state, "nope").expect_err("missing trigger should fail");

        assert!(err.to_string().contains("not found"));
    }
}
