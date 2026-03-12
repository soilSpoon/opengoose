use super::support::{complete_api_router, empty_request, read_json};
use axum::http::{Method, StatusCode};
use tower::ServiceExt;

// --- /api/health/ready and /api/health/live ---

#[tokio::test]
async fn api_health_ready_returns_ok_or_service_unavailable() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/health/ready"))
        .await
        .expect("request should be handled");

    // In-memory test state has no external deps failing, so should be ready.
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "health/ready should return 200 or 503, got {}",
        response.status()
    );
    let body = read_json(response).await;
    assert!(body.get("status").is_some());
}

#[tokio::test]
async fn api_health_live_returns_healthy() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/health/live"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.get("status").is_some());
}

// --- /api/channel-metrics ---

#[tokio::test]
async fn api_channel_metrics_returns_platforms_map() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/channel-metrics"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.get("platforms").is_some());
    assert!(body["platforms"].is_object());
}

#[tokio::test]
async fn api_channel_metrics_empty_state_has_empty_platforms() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/channel-metrics"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    let platforms = body["platforms"].as_object().unwrap();
    assert!(platforms.is_empty());
}

// --- /api/gateways ---

#[tokio::test]
async fn api_gateways_list_returns_all_known_platforms() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/gateways"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.get("gateways").is_some());
    let gateways = body["gateways"].as_array().unwrap();
    assert!(gateways.len() >= 4, "should include at least 4 known platforms");

    let names: Vec<&str> = gateways
        .iter()
        .map(|g| g["platform"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"discord"));
    assert!(names.contains(&"slack"));
    assert!(names.contains(&"telegram"));
    assert!(names.contains(&"matrix"));
}

#[tokio::test]
async fn api_gateways_all_disconnected_in_empty_state() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/gateways"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    let gateways = body["gateways"].as_array().unwrap();
    for gw in gateways {
        assert_eq!(
            gw["state"], "disconnected",
            "gateway {} should be disconnected",
            gw["platform"]
        );
    }
}

#[tokio::test]
async fn api_gateways_each_has_required_fields() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/gateways"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    let gateways = body["gateways"].as_array().unwrap();
    for gw in gateways {
        assert!(gw.get("platform").is_some());
        assert!(gw.get("state").is_some());
        assert!(gw.get("reconnect_count").is_some());
    }
}

// --- /api/gateways/{platform}/status ---

#[tokio::test]
async fn api_gateway_status_known_platform_returns_disconnected() {
    let response = complete_api_router()
        .oneshot(empty_request(Method::GET, "/api/gateways/discord/status"))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["platform"], "discord");
    assert_eq!(body["state"], "disconnected");
    assert_eq!(body["reconnect_count"], 0);
}

#[tokio::test]
async fn api_gateway_status_unknown_platform_returns_disconnected() {
    let response = complete_api_router()
        .oneshot(empty_request(
            Method::GET,
            "/api/gateways/no-such-platform/status",
        ))
        .await
        .expect("request should be handled");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["platform"], "no-such-platform");
    assert_eq!(body["state"], "disconnected");
}
