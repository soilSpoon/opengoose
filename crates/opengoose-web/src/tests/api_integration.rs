use crate::handlers;
use crate::handlers::dashboard::get_dashboard;
use crate::handlers::test_support::make_state;
use crate::routes;
use crate::routes::health::{
    MetricsResponse, QueueMetrics, RunMetrics, SessionMetrics, health as health_handler,
};
use crate::state::AppState;
use axum::{
    Json, Router,
    body::Body,
    body::to_bytes,
    extract::State,
    http::{Method, Request, StatusCode, Uri},
    routing::{get, post},
};
use opengoose_persistence::RunStatus;
use serde_json::Value;
use tower::ServiceExt;

async fn api_metrics(
    State(state): State<AppState>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let session_stats = state
        .session_store
        .stats()
        .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let recent_runs = state
        .orchestration_store
        .list_runs(None, 200)
        .map_err(|e| routes::api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let running = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Running)
        .count();
    let completed = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Completed)
        .count();
    let failed = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Failed)
        .count();
    let suspended = recent_runs
        .iter()
        .filter(|r| r.status == RunStatus::Suspended)
        .count();

    Ok(Json(MetricsResponse {
        sessions: SessionMetrics {
            total: session_stats.session_count,
            messages: session_stats.message_count,
        },
        queue: QueueMetrics {
            pending: 0,
            processing: 0,
            completed: 0,
            failed: 0,
            dead: 0,
        },
        runs: RunMetrics {
            running,
            completed,
            failed,
            suspended,
        },
    }))
}

fn api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route(
            "/api/sessions/{session_key}/messages",
            get(handlers::sessions::get_messages),
        )
        .route("/api/runs", get(handlers::runs::list_runs))
        .route("/api/agents", get(handlers::agents::list_agents))
        .route("/api/teams", get(handlers::teams::list_teams))
        .route("/api/workflows", get(handlers::workflows::list_workflows))
        .route(
            "/api/workflows/{name}",
            get(handlers::workflows::get_workflow),
        )
        .route(
            "/api/workflows/{name}/trigger",
            post(handlers::workflows::trigger_workflow),
        )
        .route("/api/dashboard", get(get_dashboard))
        .route("/api/metrics", get(api_metrics))
        .with_state(state)
}

fn full_api_router() -> Router {
    let state = make_state();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route(
            "/api/sessions/{session_key}/messages",
            get(handlers::sessions::get_messages),
        )
        .route("/api/runs", get(handlers::runs::list_runs))
        .route("/api/agents", get(handlers::agents::list_agents))
        .route("/api/teams", get(handlers::teams::list_teams))
        .route("/api/workflows", get(handlers::workflows::list_workflows))
        .route(
            "/api/workflows/{name}",
            get(handlers::workflows::get_workflow),
        )
        .route(
            "/api/workflows/{name}/trigger",
            post(handlers::workflows::trigger_workflow),
        )
        .route("/api/dashboard", get(get_dashboard))
        .route("/api/metrics", get(api_metrics))
        .route("/api/alerts", get(handlers::alerts::list_alerts))
        .route(
            "/api/alerts",
            axum::routing::post(handlers::alerts::create_alert),
        )
        .route(
            "/api/alerts/{name}",
            axum::routing::delete(handlers::alerts::delete_alert),
        )
        .route("/api/alerts/history", get(handlers::alerts::alert_history))
        .route(
            "/api/alerts/test",
            axum::routing::post(handlers::alerts::test_alerts),
        )
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state)
}

async fn read_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be json")
}

#[tokio::test]
async fn api_health_returns_healthy() {
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
async fn api_alerts_list_returns_empty_array() {
    let app = full_api_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/alerts"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn api_alerts_create_and_list_round_trip() {
    let app = full_api_router();

    let create_body = serde_json::json!({
        "name": "high-backlog",
        "description": "Queue backlog is too high",
        "metric": "queue_backlog",
        "condition": "gt",
        "threshold": 100.0
    });

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("create request should succeed");

    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    assert_eq!(created["name"], "high-backlog");
    assert_eq!(created["metric"], "queue_backlog");
    assert_eq!(created["condition"], "gt");
    assert_eq!(created["threshold"], 100.0);
    assert_eq!(created["enabled"], true);
    assert!(created["id"].is_string());

    let list_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/alerts"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("list request should succeed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list = read_json(list_response).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "high-backlog");
}

#[tokio::test]
async fn api_alerts_create_rejects_invalid_metric() {
    let app = full_api_router();

    let body = serde_json::json!({
        "name": "bad-metric",
        "metric": "nonexistent_metric",
        "condition": "gt",
        "threshold": 50.0
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let err = read_json(response).await;
    assert!(err["error"].as_str().unwrap().contains("unknown metric"));
}

#[tokio::test]
async fn api_alerts_create_rejects_invalid_condition() {
    let app = full_api_router();

    let body = serde_json::json!({
        "name": "bad-condition",
        "metric": "failed_runs",
        "condition": "neq",
        "threshold": 10.0
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let err = read_json(response).await;
    assert!(err["error"].as_str().unwrap().contains("unknown condition"));
}

#[tokio::test]
async fn api_alerts_delete_nonexistent_returns_not_found() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(Uri::from_static("/api/alerts/no-such-alert"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_alerts_history_returns_empty_array() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(Uri::from_static("/api/alerts/history"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn api_alerts_test_returns_metrics_and_triggered() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts/test"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.get("metrics").is_some());
    assert!(body.get("triggered").is_some());
    assert!(body["triggered"].is_array());
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

#[tokio::test]
async fn api_alerts_create_and_delete_round_trip() {
    let app = full_api_router();

    let create_body = serde_json::json!({
        "name": "delete-me",
        "metric": "failed_runs",
        "condition": "gt",
        "threshold": 5.0
    });

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("create should succeed");

    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    assert_eq!(created["name"], "delete-me");

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(Uri::from_static("/api/alerts/delete-me"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("delete should succeed");

    assert_eq!(delete_response.status(), StatusCode::OK);
    let deleted = read_json(delete_response).await;
    assert_eq!(deleted["deleted"], "delete-me");

    let gone_response = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(Uri::from_static("/api/alerts/delete-me"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("second delete should be handled");

    assert_eq!(gone_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_alerts_create_missing_required_field_returns_unprocessable() {
    let app = full_api_router();

    let body = serde_json::json!({
        "name": "incomplete",
        "metric": "failed_runs",
        "condition": "gt"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_alerts_create_malformed_json_returns_bad_request() {
    let app = full_api_router();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(Uri::from_static("/api/alerts"))
                .header("content-type", "application/json")
                .body(Body::from(b"{not valid json}".as_ref()))
                .unwrap(),
        )
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
