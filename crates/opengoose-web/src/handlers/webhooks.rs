use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use hmac::{Hmac, Mac};
use opengoose_teams::triggers::{WebhookCondition, matches_webhook_path};
use opengoose_types::EventBus;
use serde::Serialize;
use sha2::Sha256;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use super::AppError;
use crate::middleware::{RateLimitConfig, SlidingWindowRateLimiter};
use crate::state::AppState;

const DEFAULT_SIGNATURE_HEADER: &str = "X-Webhook-Signature";
const DEFAULT_TIMESTAMP_HEADER: &str = "X-Webhook-Timestamp";
const DEFAULT_TIMESTAMP_TOLERANCE_SECS: i64 = 300;
const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const GENERIC_WEBHOOK_UNAUTHORIZED_MESSAGE: &str = "invalid webhook request";

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize)]
pub struct WebhookResponse {
    pub accepted: bool,
    pub trigger: String,
}

/// POST /api/webhooks/*path — receive an inbound webhook and fire matching triggers.
///
/// Looks up all enabled `webhook_received` triggers and checks whether any
/// match the incoming path (prefix match). If a trigger has a `secret`
/// configured in its condition, the caller must supply it in the
/// `X-Webhook-Secret` request header. If a trigger has an `hmac_secret`,
/// the caller must also provide a valid HMAC-SHA256 signature over
/// `timestamp.body`, plus a timestamp inside the allowed replay window.
pub async fn receive_webhook(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), AppError> {
    let provided_secret = headers
        .get("X-Webhook-Secret")
        .and_then(|value| value.to_str().ok());

    let normalized = if path.starts_with('/') {
        path.clone()
    } else {
        format!("/{path}")
    };

    let triggers = state.trigger_store.list_by_type("webhook_received")?;

    let matching: Vec<_> = triggers
        .into_iter()
        .filter(|trigger| matches_webhook_path(&trigger.condition_json, &normalized))
        .collect();

    if matching.is_empty() {
        return Err(AppError::NotFound(format!(
            "no webhook trigger configured for path {normalized}"
        )));
    }

    let mut validated: Vec<(String, WebhookCondition)> = Vec::with_capacity(matching.len());

    for trigger in &matching {
        let condition: WebhookCondition =
            serde_json::from_str(&trigger.condition_json).unwrap_or_default();

        validate_plaintext_secret(&condition, provided_secret, &normalized, &trigger.name)?;
        validate_hmac_signature(&condition, &headers, &body, &normalized, &trigger.name)?;

        validated.push((trigger.name.clone(), condition));
    }

    for (trigger_name, condition) in &validated {
        validate_webhook_rate_limit(&state.webhook_rate_limiter, trigger_name, condition)?;
    }

    let fired_name = matching[0].name.clone();

    for trigger in matching {
        let db = state.db.clone();
        let trigger_store = state.trigger_store.clone();
        let team_name = trigger.team_name.clone();
        let trigger_name = trigger.name.clone();
        let trigger_input = if trigger.input.is_empty() {
            format!("Triggered by incoming webhook at {normalized}")
        } else {
            trigger.input.clone()
        };

        tokio::spawn(async move {
            let event_bus = EventBus::new(256);
            info!(
                trigger = %trigger_name,
                team = %team_name,
                "firing webhook-received trigger"
            );
            match opengoose_teams::run_headless(&team_name, &trigger_input, db, event_bus).await {
                Ok((run_id, _)) => {
                    info!(trigger = %trigger_name, run_id, "webhook-triggered team run completed");
                }
                Err(error) => {
                    error!(
                        trigger = %trigger_name,
                        team = %team_name,
                        %error,
                        "webhook-triggered team run failed"
                    );
                }
            }
            if let Err(error) = trigger_store.mark_fired(&trigger_name) {
                error!(trigger = %trigger_name, %error, "failed to mark webhook trigger as fired");
            }
        });
    }

    Ok((
        StatusCode::OK,
        Json(WebhookResponse {
            accepted: true,
            trigger: fired_name,
        }),
    ))
}

fn validate_plaintext_secret(
    condition: &WebhookCondition,
    provided_secret: Option<&str>,
    normalized_path: &str,
    trigger_name: &str,
) -> Result<(), AppError> {
    let Some(expected) = condition.secret.as_deref() else {
        return Ok(());
    };

    match provided_secret {
        Some(secret) if secret == expected => Ok(()),
        _ => Err(invalid_webhook_request(
            normalized_path,
            trigger_name,
            "invalid webhook secret",
        )),
    }
}

fn validate_hmac_signature(
    condition: &WebhookCondition,
    headers: &HeaderMap,
    body: &[u8],
    normalized_path: &str,
    trigger_name: &str,
) -> Result<(), AppError> {
    let Some(secret) = condition.hmac_secret.as_deref() else {
        return Ok(());
    };

    let signature_header = condition
        .signature_header
        .as_deref()
        .unwrap_or(DEFAULT_SIGNATURE_HEADER);
    let timestamp_header = condition
        .timestamp_header
        .as_deref()
        .unwrap_or(DEFAULT_TIMESTAMP_HEADER);
    let tolerance_secs = condition
        .timestamp_tolerance_secs
        .unwrap_or(DEFAULT_TIMESTAMP_TOLERANCE_SECS)
        .max(0);

    let timestamp = header_value(headers, timestamp_header).ok_or_else(|| {
        invalid_webhook_request(normalized_path, trigger_name, "missing webhook timestamp")
    })?;
    let timestamp_epoch = timestamp.parse::<i64>().map_err(|_| {
        invalid_webhook_request(normalized_path, trigger_name, "invalid webhook timestamp")
    })?;
    let age_secs = (Utc::now().timestamp() - timestamp_epoch).abs();
    if age_secs > tolerance_secs {
        return Err(invalid_webhook_request(
            normalized_path,
            trigger_name,
            "webhook timestamp outside replay window",
        ));
    }

    let provided_signature = header_value(headers, signature_header).ok_or_else(|| {
        invalid_webhook_request(normalized_path, trigger_name, "missing webhook signature")
    })?;
    let provided_signature = provided_signature
        .strip_prefix("sha256=")
        .unwrap_or(provided_signature);
    let provided_bytes = hex::decode(provided_signature).map_err(|_| {
        invalid_webhook_request(normalized_path, trigger_name, "invalid webhook signature")
    })?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| AppError::Internal(format!("invalid webhook signing key: {error}")))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);

    mac.verify_slice(&provided_bytes).map_err(|_| {
        invalid_webhook_request(normalized_path, trigger_name, "invalid webhook signature")
    })
}

fn validate_webhook_rate_limit(
    limiter: &SlidingWindowRateLimiter,
    trigger_name: &str,
    condition: &WebhookCondition,
) -> Result<(), AppError> {
    let config = webhook_rate_limit_config(condition);
    let (_, retry_after) = limiter.check_key_with_config(trigger_name, &config, Instant::now());
    if let Some(retry_after) = retry_after {
        warn!(
            trigger = %trigger_name,
            limit = %config.max_requests,
            retry_after = retry_after,
            window_secs = %config.window.as_secs(),
            "webhook rate limit exceeded"
        );
        return Err(AppError::TooManyRequests(format!(
            "too many webhook requests; retry after {retry_after}s"
        )));
    }

    Ok(())
}

fn webhook_rate_limit_config(condition: &WebhookCondition) -> RateLimitConfig {
    let default = RateLimitConfig::default();
    RateLimitConfig {
        max_requests: condition.rate_limit.unwrap_or(default.max_requests),
        window: Duration::from_secs(
            condition
                .rate_limit_window_secs
                .unwrap_or(DEFAULT_RATE_LIMIT_WINDOW_SECS),
        ),
    }
}

fn header_value<'a>(headers: &'a HeaderMap, header_name: &str) -> Option<&'a str> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
}

fn invalid_webhook_request(normalized_path: &str, trigger_name: &str, message: &str) -> AppError {
    warn!(
        path = %normalized_path,
        trigger = %trigger_name,
        reason = %message,
        "webhook signature validation failed"
    );
    AppError::Unauthorized(GENERIC_WEBHOOK_UNAUTHORIZED_MESSAGE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use tower::ServiceExt;

    use crate::handlers::test_support::make_state;
    use crate::state::AppState;

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
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let payload = std::str::from_utf8(&body).unwrap();
        assert!(payload.contains(GENERIC_WEBHOOK_UNAUTHORIZED_MESSAGE));
    }

    #[tokio::test]
    async fn test_rate_limited_webhook_returns_429() {
        let state = make_state();
        state
            .trigger_store
            .create(
                "rate-limited-hook",
                "webhook_received",
                r#"{"path":"/rate","rate_limit_max_requests":1}"#,
                "my-team",
                "",
            )
            .unwrap();

        let app = router(state);

        let resp = app
            .clone()
            .oneshot(request("POST", "/api/webhooks/rate", "{}"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(request("POST", "/api/webhooks/rate", "{}"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let payload = std::str::from_utf8(&body).unwrap();
        assert!(payload.contains("too many requests"));
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
                r#"{"path":"/signed","hmac_secret":"sig-secret"}"#,
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
                r#"{"path":"/signed","hmac_secret":"sig-secret"}"#,
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
                r#"{"path":"/signed","hmac_secret":"sig-secret","timestamp_tolerance_secs":10}"#,
                "my-team",
                "",
            )
            .unwrap();

        let app = router(state);
        let timestamp = (Utc::now().timestamp() - 60).to_string();
        let body = r#"{"event":"push"}"#;
        let signature = signed_signature("sig-secret", &timestamp, body.as_bytes());

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
                r#"{"path":"/signed","hmac_secret":"sig-secret"}"#,
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
}
