//! Integration tests for core API endpoints: health probes, dashboard,
//! metrics, sessions, runs, teams, and workflows.

use super::{api_router, full_api_router, read_json};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, Uri},
};
use tower::ServiceExt;

#[tokio::test]
async fn api_health_returns_ok() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/health"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = read_json(response).await;
    assert_eq!(payload["status"], "healthy");
    assert!(payload["components"]["gateways"].is_object());
}

#[tokio::test]
async fn api_ready_and_live_return_probe_payloads() {
    let app = api_router();

    let ready = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/health/ready"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("ready request should be handled");

    assert_eq!(ready.status(), StatusCode::OK);
    let ready_body = read_json(ready).await;
    assert_eq!(ready_body["status"], "healthy");

    let live = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/health/live"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("live request should be handled");

    assert_eq!(live.status(), StatusCode::OK);
    let live_body = read_json(live).await;
    assert_eq!(live_body["status"], "healthy");
    assert!(live_body.get("checked_at").is_some());
}

#[tokio::test]
async fn api_dashboard_and_metrics_return_object_payloads() {
    let app = api_router();
    let dashboard = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/dashboard"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("dashboard request should succeed");

    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard_body = read_json(dashboard).await;
    assert!(dashboard_body.get("session_count").is_some());

    let metrics = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/metrics"))
                .body(Body::empty())
                .unwrap(),
        )
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
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/sessions?limit=10"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("sessions request should succeed");
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions_body = read_json(sessions).await;
    assert!(sessions_body.is_array());

    let runs = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/runs?limit=10"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("runs request should succeed");
    assert_eq!(runs.status(), StatusCode::OK);
    let runs_body = read_json(runs).await;
    assert!(runs_body.is_array());

    let teams = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/teams"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("teams request should succeed");
    assert_eq!(teams.status(), StatusCode::OK);
    let teams_body = read_json(teams).await;
    assert!(teams_body.is_array());

    let workflows = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/workflows"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("workflows request should succeed");
    assert_eq!(workflows.status(), StatusCode::OK);
    let workflows_body = read_json(workflows).await;
    assert!(workflows_body.is_array());
}

#[tokio::test]
async fn api_session_messages_returns_empty_array_for_missing_session() {
    let app = api_router();
    let messages = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static(
                    "/api/sessions/discord%3Aguild%3Achannel/messages?limit=5",
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("messages request should succeed");

    assert_eq!(messages.status(), StatusCode::OK);
    let body = read_json(messages).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_session_export_returns_not_found_for_missing_session() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static(
                    "/api/sessions/discord%3Aguild%3Achannel/export?format=json",
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("export request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_batch_session_export_returns_json_object() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static(
                    "/api/sessions/export?since=7d&format=json",
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("batch export request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_object());
    assert!(body.get("sessions").is_some());
}

// ── Fallback / 404 test ───────────────────────────────────────────────

#[tokio::test]
async fn api_unknown_route_returns_not_found() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/does-not-exist"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_missing_workflow_trigger_returns_not_found() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/workflows/no-such-workflow/trigger"))
                .header("content-type", "application/json")
                .body(Body::from(br#"{"input":"run"}"#.to_vec()))
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Sessions handler edge cases ──────────────────────────────────────

#[tokio::test]
async fn api_sessions_list_with_explicit_limit_returns_array() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/sessions?limit=5"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_sessions_invalid_limit_returns_bad_request() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/sessions?limit=abc"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_session_messages_invalid_limit_returns_bad_request() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static(
                    "/api/sessions/discord%3Aguild%3Achannel/messages?limit=notanumber",
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_batch_session_export_without_range_returns_unprocessable() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/sessions/export?format=json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("batch export request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Runs handler edge cases ──────────────────────────────────────────

#[tokio::test]
async fn api_runs_with_running_status_filter_returns_array() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/runs?status=running"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_runs_with_completed_status_filter_returns_array() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/runs?status=completed"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
}

#[tokio::test]
async fn api_runs_invalid_status_filter_returns_unprocessable() {
    // Invalid status values are rejected by input validation (OPE-67).
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/runs?status=not_a_real_status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_runs_invalid_limit_returns_bad_request() {
    let app = api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/runs?limit=notanumber"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
