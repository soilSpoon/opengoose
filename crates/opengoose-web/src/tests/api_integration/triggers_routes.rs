use super::support::{complete_api_router, empty_request, json_request, read_json};
use axum::http::{Method, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn api_triggers_list_returns_empty_array() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/triggers"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn api_triggers_create_and_list_round_trip() {
    let app = complete_api_router();

    let create_response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "my-webhook",
                "trigger_type": "webhook_received",
                "team_name": "ops-team",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("create request should succeed");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    assert_eq!(created["name"], "my-webhook");
    assert_eq!(created["trigger_type"], "webhook_received");
    assert_eq!(created["team_name"], "ops-team");
    assert_eq!(created["enabled"], true);

    let list_response = app
        .oneshot(empty_request(Method::GET, "/api/triggers"))
        .await
        .expect("list request should succeed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list = read_json(list_response).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "my-webhook");
}

#[tokio::test]
async fn api_triggers_get_returns_created_trigger() {
    let app = complete_api_router();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "cron-daily",
                "trigger_type": "cron",
                "team_name": "infra-team",
                "input": "daily-run",
                "condition_json": null
            }),
        ))
        .await
        .expect("create should succeed");

    let get_response = app
        .oneshot(empty_request(Method::GET, "/api/triggers/cron-daily"))
        .await
        .expect("get request should succeed");

    assert_eq!(get_response.status(), StatusCode::OK);
    let body = read_json(get_response).await;
    assert_eq!(body["name"], "cron-daily");
    assert_eq!(body["trigger_type"], "cron");
    assert_eq!(body["team_name"], "infra-team");
    assert_eq!(body["input"], "daily-run");
}

#[tokio::test]
async fn api_triggers_get_nonexistent_returns_not_found() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/triggers/no-such-trigger"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_triggers_update_modifies_trigger() {
    let app = complete_api_router();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "updatable",
                "trigger_type": "webhook_received",
                "team_name": "team-a",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("create should succeed");

    let update_response = app
        .clone()
        .oneshot(json_request(
            Method::PUT,
            "/api/triggers/updatable",
            serde_json::json!({
                "trigger_type": "message_received",
                "team_name": "team-b",
                "input": "updated-input",
                "condition_json": null
            }),
        ))
        .await
        .expect("update should succeed");

    assert_eq!(update_response.status(), StatusCode::OK);
    let updated = read_json(update_response).await;
    assert_eq!(updated["name"], "updatable");
    assert_eq!(updated["trigger_type"], "message_received");
    assert_eq!(updated["team_name"], "team-b");
    assert_eq!(updated["input"], "updated-input");
}

#[tokio::test]
async fn api_triggers_update_nonexistent_returns_not_found() {
    let response = complete_api_router()
        .oneshot(json_request(
            Method::PUT,
            "/api/triggers/no-such-trigger",
            serde_json::json!({
                "trigger_type": "cron",
                "team_name": "team-x",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_triggers_delete_returns_deleted_name() {
    let app = complete_api_router();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "delete-me",
                "trigger_type": "cron",
                "team_name": "team-c",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("create should succeed");

    let delete_response = app
        .clone()
        .oneshot(empty_request(Method::DELETE, "/api/triggers/delete-me"))
        .await
        .expect("delete should succeed");

    assert_eq!(delete_response.status(), StatusCode::OK);
    let body = read_json(delete_response).await;
    assert_eq!(body["deleted"], "delete-me");

    let gone_response = app
        .oneshot(empty_request(Method::DELETE, "/api/triggers/delete-me"))
        .await
        .expect("second delete should be handled");

    assert_eq!(gone_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_triggers_delete_nonexistent_returns_not_found() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::DELETE, "/api/triggers/ghost"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_triggers_set_enabled_false_and_back() {
    let app = complete_api_router();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "toggle-me",
                "trigger_type": "webhook_received",
                "team_name": "team-d",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("create should succeed");

    let disable_response = app
        .clone()
        .oneshot(json_request(
            Method::PATCH,
            "/api/triggers/toggle-me/enabled",
            serde_json::json!({ "enabled": false }),
        ))
        .await
        .expect("disable should succeed");

    assert_eq!(disable_response.status(), StatusCode::OK);
    let body = read_json(disable_response).await;
    assert_eq!(body["name"], "toggle-me");
    assert_eq!(body["enabled"], false);

    let enable_response = app
        .oneshot(json_request(
            Method::PATCH,
            "/api/triggers/toggle-me/enabled",
            serde_json::json!({ "enabled": true }),
        ))
        .await
        .expect("re-enable should succeed");

    assert_eq!(enable_response.status(), StatusCode::OK);
    let body = read_json(enable_response).await;
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn api_triggers_set_enabled_nonexistent_returns_not_found() {
    let response = complete_api_router()
        .oneshot(json_request(
            Method::PATCH,
            "/api/triggers/ghost/enabled",
            serde_json::json!({ "enabled": false }),
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_triggers_create_invalid_condition_json_returns_bad_request() {
    let response = complete_api_router()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "bad-condition",
                "trigger_type": "webhook_received",
                "team_name": "team-e",
                "input": null,
                "condition_json": "{not valid json}"
            }),
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_triggers_create_empty_name_returns_unprocessable() {
    let response = complete_api_router()
        .oneshot(json_request(
            Method::POST,
            "/api/triggers",
            serde_json::json!({
                "name": "   ",
                "trigger_type": "webhook_received",
                "team_name": "team-f",
                "input": null,
                "condition_json": null
            }),
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
