use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::{
    CreateTriggerRequest, SetEnabledRequest, TestTriggerRequest, UpdateTriggerRequest,
    create_trigger, delete_trigger, get_trigger, list_triggers, set_trigger_enabled, test_trigger,
    update_trigger,
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
    assert!(matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`trigger_type`")));
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

    let Json(trigger) = get_trigger(State(state.clone()), Path("my-hook".into()))
        .await
        .expect("get should succeed");
    assert_eq!(trigger.name, "my-hook");

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
    assert!(matches!(err, WebError::UnprocessableEntity(msg) if msg.contains("`trigger_type`")));
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
