use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::error;

use opengoose_persistence::Trigger;
use opengoose_types::EventBus;

use super::AppError;
use crate::state::AppState;

// ── Response types ─────────────────────────────────────────────────────────────

/// JSON representation of a trigger.
#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
    pub fire_count: i32,
    pub last_fired_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Trigger> for TriggerResponse {
    fn from(t: Trigger) -> Self {
        Self {
            name: t.name,
            trigger_type: t.trigger_type,
            condition_json: t.condition_json,
            team_name: t.team_name,
            input: t.input,
            enabled: t.enabled,
            fire_count: t.fire_count,
            last_fired_at: t.last_fired_at,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

// ── Request types ──────────────────────────────────────────────────────────────

/// JSON body for creating a new trigger.
#[derive(Deserialize)]
pub struct CreateTriggerRequest {
    pub name: String,
    /// One of: webhook_received, file_watch, cron, message_received
    pub trigger_type: String,
    /// JSON object with type-specific condition fields.
    pub condition_json: Option<String>,
    pub team_name: String,
    pub input: Option<String>,
}

/// JSON body for updating an existing trigger.
#[derive(Deserialize)]
pub struct UpdateTriggerRequest {
    pub trigger_type: String,
    pub condition_json: Option<String>,
    pub team_name: String,
    pub input: Option<String>,
}

/// JSON body for enabling or disabling a trigger.
#[derive(Deserialize)]
pub struct SetEnabledRequest {
    pub enabled: bool,
}

/// JSON body for firing a test event.
#[derive(Deserialize, Default)]
pub struct TestTriggerRequest {
    pub input: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

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
        .ok_or_else(|| AppError::NotFound(format!("trigger `{name}` not found")))?;
    Ok(Json(TriggerResponse::from(trigger)))
}

/// POST /api/triggers — create a new trigger.
pub async fn create_trigger(
    State(state): State<AppState>,
    Json(body): Json<CreateTriggerRequest>,
) -> Result<(StatusCode, Json<TriggerResponse>), AppError> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`name` must not be empty".into(),
        ));
    }
    let team_name = body.team_name.trim().to_string();
    if team_name.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`team_name` must not be empty".into(),
        ));
    }
    let trigger_type = body.trigger_type.trim().to_string();
    if trigger_type.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`trigger_type` must not be empty".into(),
        ));
    }
    let condition_json = body.condition_json.unwrap_or_else(|| "{}".into());
    let input = body.input.unwrap_or_default();

    // Validate condition_json is valid JSON.
    serde_json::from_str::<serde_json::Value>(&condition_json)
        .map_err(|e| AppError::BadRequest(format!("`condition_json` is not valid JSON: {e}")))?;

    let trigger =
        state
            .trigger_store
            .create(&name, &trigger_type, &condition_json, &team_name, &input)?;

    Ok((StatusCode::CREATED, Json(TriggerResponse::from(trigger))))
}

/// PUT /api/triggers/:name — update mutable fields of a trigger.
pub async fn update_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<UpdateTriggerRequest>,
) -> Result<Json<TriggerResponse>, AppError> {
    let trigger_type = body.trigger_type.trim().to_string();
    if trigger_type.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`trigger_type` must not be empty".into(),
        ));
    }
    let team_name = body.team_name.trim().to_string();
    if team_name.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "`team_name` must not be empty".into(),
        ));
    }
    let condition_json = body.condition_json.unwrap_or_else(|| "{}".into());
    let input = body.input.unwrap_or_default();

    serde_json::from_str::<serde_json::Value>(&condition_json)
        .map_err(|e| AppError::BadRequest(format!("`condition_json` is not valid JSON: {e}")))?;

    let trigger = state
        .trigger_store
        .update(&name, &trigger_type, &condition_json, &team_name, &input)?
        .ok_or_else(|| AppError::NotFound(format!("trigger `{name}` not found")))?;

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
        Err(AppError::NotFound(format!("trigger `{name}` not found")))
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
        Err(AppError::NotFound(format!("trigger `{name}` not found")))
    }
}

/// POST /api/triggers/:name/test — fire a test run for the trigger's workflow.
pub async fn test_trigger(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<TestTriggerRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let trigger = state
        .trigger_store
        .get_by_name(&name)?
        .ok_or_else(|| AppError::NotFound(format!("trigger `{name}` not found")))?;

    let input = body
        .and_then(|Json(payload)| payload.input)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if trigger.input.is_empty() {
                format!("Test run fired from the web dashboard for trigger {name}")
            } else {
                trigger.input.clone()
            }
        });

    let db = state.db.clone();
    let trigger_store = state.trigger_store.clone();
    let team_name = trigger.team_name.clone();
    let trigger_name = trigger.name.clone();
    let run_input = input.clone();

    tokio::spawn(async move {
        let event_bus = EventBus::new(256);
        match opengoose_teams::run_headless(&team_name, &run_input, db, event_bus).await {
            Ok((run_id, _)) => {
                if let Err(e) = trigger_store.mark_fired(&trigger_name) {
                    error!(trigger = %trigger_name, %e, "failed to mark trigger fired after test");
                } else {
                    tracing::info!(trigger = %trigger_name, run_id, "test trigger run completed");
                }
            }
            Err(e) => {
                error!(trigger = %trigger_name, team = %team_name, %e, "test trigger run failed");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "accepted": true,
            "trigger": name,
            "team": trigger.team_name,
            "input": input,
        })),
    ))
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;

    use super::{
        CreateTriggerRequest, SetEnabledRequest, TestTriggerRequest, UpdateTriggerRequest,
        create_trigger, delete_trigger, get_trigger, list_triggers, set_trigger_enabled,
        test_trigger, update_trigger,
    };
    use crate::error::WebError;
    use crate::handlers::test_support::make_state;

    #[tokio::test]
    async fn list_triggers_returns_empty_vec_initially() {
        let Json(items) = list_triggers(State(make_state()))
            .await
            .expect("list should succeed");
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn create_and_list_trigger() {
        let state = make_state();

        let (status, Json(created)) = create_trigger(
            State(state.clone()),
            Json(CreateTriggerRequest {
                name: "on-pr".into(),
                trigger_type: "webhook_received".into(),
                condition_json: Some(r#"{"path":"/github"}"#.into()),
                team_name: "review-team".into(),
                input: Some("review the PR".into()),
            }),
        )
        .await
        .expect("create should succeed");

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created.name, "on-pr");
        assert_eq!(created.trigger_type, "webhook_received");
        assert_eq!(created.team_name, "review-team");
        assert!(created.enabled);

        let Json(items) = list_triggers(State(state))
            .await
            .expect("list should succeed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "on-pr");
    }

    #[tokio::test]
    async fn create_trigger_defaults_condition_json_to_empty_object() {
        let state = make_state();

        let (_, Json(created)) = create_trigger(
            State(state),
            Json(CreateTriggerRequest {
                name: "no-condition".into(),
                trigger_type: "file_watch".into(),
                condition_json: None,
                team_name: "my-team".into(),
                input: None,
            }),
        )
        .await
        .expect("create should succeed");

        assert_eq!(created.condition_json, "{}");
        assert_eq!(created.input, "");
    }

    #[tokio::test]
    async fn create_trigger_trims_name_team_and_trigger_type() {
        let state = make_state();

        let (_, Json(created)) = create_trigger(
            State(state),
            Json(CreateTriggerRequest {
                name: "  on-pr  ".into(),
                trigger_type: " webhook_received ".into(),
                condition_json: None,
                team_name: " review-team ".into(),
                input: Some("review the PR".into()),
            }),
        )
        .await
        .expect("create should succeed");

        assert_eq!(created.name, "on-pr");
        assert_eq!(created.trigger_type, "webhook_received");
        assert_eq!(created.team_name, "review-team");
    }

    #[tokio::test]
    async fn create_trigger_rejects_blank_name() {
        let err = create_trigger(
            State(make_state()),
            Json(CreateTriggerRequest {
                name: "  ".into(),
                trigger_type: "webhook_received".into(),
                condition_json: None,
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("blank name should be rejected");
        assert!(matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`name`")));
    }

    #[tokio::test]
    async fn create_trigger_rejects_blank_team_name() {
        let err = create_trigger(
            State(make_state()),
            Json(CreateTriggerRequest {
                name: "on-pr".into(),
                trigger_type: "webhook_received".into(),
                condition_json: None,
                team_name: "   ".into(),
                input: None,
            }),
        )
        .await
        .expect_err("blank team name should be rejected");
        assert!(matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`team_name`")));
    }

    #[tokio::test]
    async fn create_trigger_rejects_blank_trigger_type() {
        let err = create_trigger(
            State(make_state()),
            Json(CreateTriggerRequest {
                name: "on-pr".into(),
                trigger_type: "   ".into(),
                condition_json: None,
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("blank trigger type should be rejected");
        assert!(
            matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`trigger_type`"))
        );
    }

    #[tokio::test]
    async fn create_trigger_rejects_invalid_condition_json() {
        let err = create_trigger(
            State(make_state()),
            Json(CreateTriggerRequest {
                name: "bad-json".into(),
                trigger_type: "webhook_received".into(),
                condition_json: Some("not valid json".into()),
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("invalid JSON should be rejected");
        assert!(matches!(err, WebError::BadRequest(msg) if msg.contains("`condition_json`")));
    }

    #[tokio::test]
    async fn get_trigger_returns_trigger_and_missing_returns_404() {
        let state = make_state();
        state
            .trigger_store
            .create("my-hook", "webhook_received", "{}", "team-a", "")
            .unwrap();

        let Json(t) = get_trigger(State(state.clone()), Path("my-hook".into()))
            .await
            .expect("get should succeed");
        assert_eq!(t.name, "my-hook");

        let err = get_trigger(State(state), Path("no-such".into()))
            .await
            .expect_err("missing trigger should return error");
        assert!(matches!(err, WebError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_trigger_patches_fields() {
        let state = make_state();
        state
            .trigger_store
            .create(
                "my-hook",
                "webhook_received",
                r#"{"path":"/old"}"#,
                "team-a",
                "old input",
            )
            .unwrap();

        let Json(updated) = update_trigger(
            State(state.clone()),
            Path("my-hook".into()),
            Json(UpdateTriggerRequest {
                trigger_type: "file_watch".into(),
                condition_json: Some(r#"{"path":"/new"}"#.into()),
                team_name: "team-b".into(),
                input: Some("new input".into()),
            }),
        )
        .await
        .expect("update should succeed");

        assert_eq!(updated.trigger_type, "file_watch");
        assert_eq!(updated.team_name, "team-b");
        assert_eq!(updated.input, "new input");
    }

    #[tokio::test]
    async fn update_trigger_trims_fields_and_defaults_optional_values() {
        let state = make_state();
        state
            .trigger_store
            .create(
                "my-hook",
                "webhook_received",
                r#"{"path":"/old"}"#,
                "team-a",
                "old input",
            )
            .unwrap();

        let Json(updated) = update_trigger(
            State(state),
            Path("my-hook".into()),
            Json(UpdateTriggerRequest {
                trigger_type: " file_watch ".into(),
                condition_json: None,
                team_name: " team-b ".into(),
                input: None,
            }),
        )
        .await
        .expect("update should succeed");

        assert_eq!(updated.trigger_type, "file_watch");
        assert_eq!(updated.team_name, "team-b");
        assert_eq!(updated.condition_json, "{}");
        assert_eq!(updated.input, "");
    }

    #[tokio::test]
    async fn update_trigger_rejects_blank_team_name() {
        let err = update_trigger(
            State(make_state()),
            Path("my-hook".into()),
            Json(UpdateTriggerRequest {
                trigger_type: "webhook_received".into(),
                condition_json: None,
                team_name: "   ".into(),
                input: None,
            }),
        )
        .await
        .expect_err("blank team name should fail");
        assert!(matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`team_name`")));
    }

    #[tokio::test]
    async fn update_trigger_rejects_blank_trigger_type() {
        let err = update_trigger(
            State(make_state()),
            Path("my-hook".into()),
            Json(UpdateTriggerRequest {
                trigger_type: "   ".into(),
                condition_json: None,
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("blank trigger type should fail");
        assert!(
            matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`trigger_type`"))
        );
    }

    #[tokio::test]
    async fn update_trigger_rejects_invalid_condition_json() {
        let err = update_trigger(
            State(make_state()),
            Path("my-hook".into()),
            Json(UpdateTriggerRequest {
                trigger_type: "webhook_received".into(),
                condition_json: Some("not valid json".into()),
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("invalid JSON should fail");
        assert!(matches!(err, WebError::BadRequest(msg) if msg.contains("`condition_json`")));
    }

    #[tokio::test]
    async fn update_trigger_returns_404_for_missing() {
        let err = update_trigger(
            State(make_state()),
            Path("no-such".into()),
            Json(UpdateTriggerRequest {
                trigger_type: "webhook_received".into(),
                condition_json: None,
                team_name: "team".into(),
                input: None,
            }),
        )
        .await
        .expect_err("missing trigger should fail");
        assert!(matches!(err, WebError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_trigger_removes_and_missing_returns_404() {
        let state = make_state();
        state
            .trigger_store
            .create("to-delete", "webhook_received", "{}", "team-a", "")
            .unwrap();

        let Json(result) = delete_trigger(State(state.clone()), Path("to-delete".into()))
            .await
            .expect("delete should succeed");
        assert_eq!(result["deleted"].as_str(), Some("to-delete"));

        let err = delete_trigger(State(state), Path("to-delete".into()))
            .await
            .expect_err("second delete should fail");
        assert!(matches!(err, WebError::NotFound(_)));
    }

    #[tokio::test]
    async fn set_trigger_enabled_toggles_state() {
        let state = make_state();
        state
            .trigger_store
            .create("my-hook", "webhook_received", "{}", "team-a", "")
            .unwrap();

        let Json(result) = set_trigger_enabled(
            State(state.clone()),
            Path("my-hook".into()),
            Json(SetEnabledRequest { enabled: false }),
        )
        .await
        .expect("disable should succeed");
        assert_eq!(result["enabled"].as_bool(), Some(false));

        let Json(result) = set_trigger_enabled(
            State(state),
            Path("my-hook".into()),
            Json(SetEnabledRequest { enabled: true }),
        )
        .await
        .expect("re-enable should succeed");
        assert_eq!(result["enabled"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn test_trigger_trims_explicit_input() {
        let state = make_state();
        state
            .trigger_store
            .create("my-hook", "webhook_received", "{}", "team-a", "saved input")
            .unwrap();

        let (status, Json(result)) = test_trigger(
            State(state),
            Path("my-hook".into()),
            Some(Json(TestTriggerRequest {
                input: Some("  run now  ".into()),
            })),
        )
        .await
        .expect("test trigger should succeed");

        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(result["trigger"].as_str(), Some("my-hook"));
        assert_eq!(result["team"].as_str(), Some("team-a"));
        assert_eq!(result["input"].as_str(), Some("run now"));
    }

    #[tokio::test]
    async fn test_trigger_uses_saved_input_when_body_is_missing() {
        let state = make_state();
        state
            .trigger_store
            .create("my-hook", "webhook_received", "{}", "team-a", "saved input")
            .unwrap();

        let (_, Json(result)) = test_trigger(State(state), Path("my-hook".into()), None)
            .await
            .expect("test trigger should succeed");

        assert_eq!(result["input"].as_str(), Some("saved input"));
    }

    #[tokio::test]
    async fn test_trigger_uses_default_input_when_saved_and_body_inputs_are_blank() {
        let state = make_state();
        state
            .trigger_store
            .create("my-hook", "webhook_received", "{}", "team-a", "")
            .unwrap();

        let (_, Json(result)) = test_trigger(
            State(state),
            Path("my-hook".into()),
            Some(Json(TestTriggerRequest {
                input: Some("   ".into()),
            })),
        )
        .await
        .expect("test trigger should succeed");

        assert_eq!(
            result["input"].as_str(),
            Some("Test run fired from the web dashboard for trigger my-hook")
        );
    }

    #[tokio::test]
    async fn test_trigger_returns_404_for_missing_trigger() {
        let err = test_trigger(State(make_state()), Path("no-such".into()), None)
            .await
            .expect_err("missing trigger should fail");
        assert!(matches!(err, WebError::NotFound(_)));
    }

    #[tokio::test]
    async fn set_trigger_enabled_returns_404_for_missing() {
        let err = set_trigger_enabled(
            State(make_state()),
            Path("no-such".into()),
            Json(SetEnabledRequest { enabled: false }),
        )
        .await
        .expect_err("missing trigger should fail");
        assert!(matches!(err, WebError::NotFound(_)));
    }
}
