use super::support::{complete_api_router, empty_request, read_json};
use axum::http::{Method, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn api_events_history_returns_paginated_response() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.get("items").is_some());
    assert!(body["items"].is_array());
    assert!(body.get("limit").is_some());
    assert!(body.get("offset").is_some());
    assert!(body.get("has_more").is_some());
}

#[tokio::test]
async fn api_events_history_with_explicit_limit_returns_ok() {
    let response = complete_api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/events/history?limit=10&offset=0",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body["items"].is_array());
    assert_eq!(body["limit"], 10);
    assert_eq!(body["offset"], 0);
}

#[tokio::test]
async fn api_events_history_default_limit_is_100() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["limit"], 100);
    assert_eq!(body["offset"], 0);
}

#[tokio::test]
async fn api_events_history_zero_limit_returns_unprocessable() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history?limit=0"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_events_history_limit_over_1000_returns_unprocessable() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history?limit=1001"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_events_history_negative_offset_returns_unprocessable() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history?offset=-1"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_events_history_with_gateway_filter_returns_ok() {
    let response = complete_api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/events/history?gateway=discord&limit=5",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body["items"].is_array());
}

#[tokio::test]
async fn api_events_history_with_kind_filter_returns_ok() {
    let response = complete_api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/events/history?kind=message_received",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body["items"].is_array());
}

#[tokio::test]
async fn api_events_history_has_more_is_false_when_empty() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/events/history?limit=50"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["has_more"], false);
}
