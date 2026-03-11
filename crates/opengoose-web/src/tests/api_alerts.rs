//! Integration tests for the alerts API: CRUD operations, validation, and
//! the test-alerts endpoint.

use super::{full_api_router, read_json};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, Uri},
};
use tower::ServiceExt;

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

    // Create an alert rule
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

    // List should now contain the new rule
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

    // Verify it is gone — a second delete returns 404.
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

    // `threshold` is missing — JSON body is structurally valid but
    // deserialization will fail, causing Axum to return 422.
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
    // Syntactically invalid JSON → Axum 0.8 returns 400 Bad Request.
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
