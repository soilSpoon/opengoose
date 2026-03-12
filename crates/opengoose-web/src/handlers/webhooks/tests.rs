use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tower::ServiceExt;

use super::auth::{DEFAULT_SIGNATURE_HEADER, DEFAULT_TIMESTAMP_HEADER};
use super::payload::normalize_path;
use super::receive_webhook;
use crate::handlers::test_support::make_state;
use crate::state::AppState;

/// Test-only HMAC secret — never used outside of unit tests.
const TEST_HMAC_SECRET: &str = "test-only-hmac-secret";

type HmacSha256 = Hmac<Sha256>;

fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/webhooks/{*path}", post(receive_webhook))
        .with_state(state)
}

fn signed_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("test hmac key should be valid");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn request(method: &str, uri: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(body))
        .unwrap()
}

#[test]
fn normalize_path_adds_leading_slash_when_missing() {
    assert_eq!(normalize_path("github/pr"), "/github/pr");
}

#[test]
fn normalize_path_preserves_existing_leading_slash() {
    assert_eq!(normalize_path("/github/pr"), "/github/pr");
}

#[tokio::test]
async fn test_no_trigger_returns_404() {
    let state = make_state();
    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/github/pr", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_matching_trigger_returns_200() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "test-hook",
            "webhook_received",
            r#"{"path":"/github"}"#,
            "my-team",
            "handle webhook",
        )
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/github/pr", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_valid_secret_returns_200() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "secret-hook",
            "webhook_received",
            r#"{"path":"/secure","secret":"mysecret"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/secure/payload")
        .header("X-Webhook-Secret", "mysecret")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invalid_secret_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "secret-hook",
            "webhook_received",
            r#"{"path":"/secure","secret":"mysecret"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/secure/payload")
        .header("X-Webhook-Secret", "wrongsecret")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_secret_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "secret-hook",
            "webhook_received",
            r#"{"path":"/secure","secret":"mysecret"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/secure/payload", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_valid_hmac_signature_returns_200() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            &format!(r#"{{"path":"/signed","hmac_secret":"{TEST_HMAC_SECRET}"}}"#),
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);
    let timestamp = Utc::now().timestamp().to_string();
    let body = r#"{"event":"push"}"#;
    let signature = signed_signature(TEST_HMAC_SECRET, &timestamp, body.as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/signed/payload")
        .header(DEFAULT_TIMESTAMP_HEADER, &timestamp)
        .header(DEFAULT_SIGNATURE_HEADER, signature)
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_valid_hmac_signature_with_custom_headers_returns_200() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            r#"{"path":"/signed","hmac_secret":"sig-secret","signature_header":"X-Hub-Signature-256","timestamp_header":"X-Hub-Timestamp"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);
    let timestamp = Utc::now().timestamp().to_string();
    let body = r#"{"event":"push"}"#;
    let signature = signed_signature("sig-secret", &timestamp, body.as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/signed/payload")
        .header("X-Hub-Timestamp", &timestamp)
        .header("X-Hub-Signature-256", signature)
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invalid_hmac_signature_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            &format!(r#"{{"path":"/signed","hmac_secret":"{TEST_HMAC_SECRET}"}}"#),
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);
    let timestamp = Utc::now().timestamp().to_string();
    let body = r#"{"event":"push"}"#;

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/signed/payload")
        .header(DEFAULT_TIMESTAMP_HEADER, &timestamp)
        .header(DEFAULT_SIGNATURE_HEADER, "sha256=deadbeef")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stale_hmac_timestamp_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            &format!(
                r#"{{"path":"/signed","hmac_secret":"{TEST_HMAC_SECRET}","timestamp_tolerance_secs":10}}"#
            ),
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);
    let timestamp = (Utc::now().timestamp() - 60).to_string();
    let body = r#"{"event":"push"}"#;
    let signature = signed_signature(TEST_HMAC_SECRET, &timestamp, body.as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/signed/payload")
        .header(DEFAULT_TIMESTAMP_HEADER, &timestamp)
        .header(DEFAULT_SIGNATURE_HEADER, signature)
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_hmac_headers_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            &format!(r#"{{"path":"/signed","hmac_secret":"{TEST_HMAC_SECRET}"}}"#),
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/signed/payload", "{}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_custom_hmac_headers_returns_401() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "signed-hook",
            "webhook_received",
            r#"{"path":"/signed","hmac_secret":"sig-secret","signature_header":"X-Hub-Signature-256","timestamp_header":"X-Hub-Timestamp"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let timestamp = Utc::now().timestamp().to_string();
    let body = r#"{"event":"push"}"#;
    let signature = signed_signature("sig-secret", &timestamp, body.as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/webhooks/signed/payload")
        .header("X-Hub-Timestamp", &timestamp)
        .header(DEFAULT_SIGNATURE_HEADER, signature)
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_path_no_match_returns_404() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "github-hook",
            "webhook_received",
            r#"{"path":"/github"}"#,
            "my-team",
            "",
        )
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/gitlab/push", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_disabled_trigger_not_matched() {
    let state = make_state();
    state
        .trigger_store
        .create(
            "disabled-hook",
            "webhook_received",
            r#"{"path":"/events"}"#,
            "my-team",
            "",
        )
        .unwrap();
    state
        .trigger_store
        .set_enabled("disabled-hook", false)
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/events", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_no_path_condition_matches_any() {
    let state = make_state();
    state
        .trigger_store
        .create("catch-all-hook", "webhook_received", r#"{}"#, "my-team", "")
        .unwrap();

    let app = router(state);

    let resp = app
        .oneshot(request("POST", "/api/webhooks/anything/at/all", ""))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
