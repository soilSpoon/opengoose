use super::support::{api_router, empty_request, full_api_router, json_request, read_json};
use axum::http::{Method, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn api_health_returns_healthy() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/health"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = read_json(response).await;
    assert_eq!(payload["status"], "healthy");
}

#[tokio::test]
async fn api_dashboard_and_metrics_return_object_payloads() {
    let app = api_router();
    let dashboard = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/dashboard"))
        .await
        .expect("dashboard request should succeed");

    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard_body = read_json(dashboard).await;
    assert!(dashboard_body.get("session_count").is_some());

    let metrics = app
        .oneshot(empty_request(Method::GET, "/api/metrics"))
        .await
        .expect("metrics request should succeed");

    assert_eq!(metrics.status(), StatusCode::OK);
    let metrics_body = read_json(metrics).await;
    assert!(metrics_body.get("sessions").is_some());
    assert!(metrics_body.get("runs").is_some());
}

#[tokio::test]
async fn api_session_and_run_lists_are_arrays() {
    let app = api_router();

    let sessions = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/sessions?limit=10"))
        .await
        .expect("sessions request should succeed");
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions_body = read_json(sessions).await;
    assert!(sessions_body.is_array());

    let runs = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/runs?limit=10"))
        .await
        .expect("runs request should succeed");
    assert_eq!(runs.status(), StatusCode::OK);
    let runs_body = read_json(runs).await;
    assert!(runs_body.is_array());

    let teams = app
        .clone()
        .oneshot(empty_request(Method::GET, "/api/teams"))
        .await
        .expect("teams request should succeed");
    assert_eq!(teams.status(), StatusCode::OK);
    let teams_body = read_json(teams).await;
    assert!(teams_body.is_array());

    let workflows = app
        .oneshot(empty_request(Method::GET, "/api/workflows"))
        .await
        .expect("workflows request should succeed");
    assert_eq!(workflows.status(), StatusCode::OK);
    let workflows_body = read_json(workflows).await;
    assert!(workflows_body.is_array());
}

#[tokio::test]
async fn api_session_messages_returns_empty_array_for_missing_session() {
    let response = api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/sessions/discord%3Aguild%3Achannel/messages?limit=5",
        ))
        .await
        .expect("messages request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_missing_workflow_trigger_returns_not_found() {
    let response = full_api_router()
        .oneshot(json_request(
            Method::POST,
            "/api/workflows/no-such-workflow/trigger",
            serde_json::json!({ "input": "run" }),
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_unknown_route_returns_not_found() {
    let response = full_api_router()
        .oneshot(empty_request(Method::GET, "/api/does-not-exist"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_sessions_list_with_explicit_limit_returns_array() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/sessions?limit=5"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_sessions_invalid_limit_returns_bad_request() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/sessions?limit=abc"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_session_messages_invalid_limit_returns_bad_request() {
    let response = api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/sessions/discord%3Aguild%3Achannel/messages?limit=notanumber",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_runs_with_running_status_filter_returns_array() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/runs?status=running"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_runs_with_completed_status_filter_returns_array() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/runs?status=completed"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_runs_invalid_status_filter_returns_unprocessable() {
    let response = api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/runs?status=not_a_real_status",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_runs_invalid_limit_returns_bad_request() {
    let response = api_router()
        .oneshot(empty_request(Method::GET, "/api/runs?limit=notanumber"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
