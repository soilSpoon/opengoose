use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use opengoose_teams::triggers::{WebhookCondition, matches_webhook_path};
use opengoose_types::EventBus;
use serde::Serialize;
use tracing::{error, info, warn};

use super::AppError;
use crate::state::AppState;

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
/// `X-Webhook-Secret` request header. Returns 200 on success, 404 if no
/// trigger matches, and 401 if the secret is missing or wrong.
pub async fn receive_webhook(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<WebhookResponse>), AppError> {
    let provided_secret = headers
        .get("X-Webhook-Secret")
        .and_then(|v| v.to_str().ok());

    // Normalize path so matching is consistent regardless of leading slash.
    let normalized = if path.starts_with('/') {
        path.clone()
    } else {
        format!("/{path}")
    };

    let triggers = state.trigger_store.list_by_type("webhook_received")?;

    let matching: Vec<_> = triggers
        .into_iter()
        .filter(|t| matches_webhook_path(&t.condition_json, &normalized))
        .collect();

    if matching.is_empty() {
        return Err(AppError::NotFound(format!(
            "no webhook trigger configured for path /{path}"
        )));
    }

    // Validate secret for every matched trigger that has one configured.
    for trigger in &matching {
        let cond: WebhookCondition =
            serde_json::from_str(&trigger.condition_json).unwrap_or_default();
        if let Some(ref expected) = cond.secret {
            match provided_secret {
                Some(s) if s == expected => {}
                _ => {
                    warn!(
                        path = %normalized,
                        trigger = %trigger.name,
                        "webhook secret invalid or missing"
                    );
                    return Err(AppError::Unauthorized(
                        "invalid or missing webhook secret".into(),
                    ));
                }
            }
        }
    }

    let fired_name = matching[0].name.clone();

    for trigger in matching {
        let db = state.db.clone();
        let trigger_store = state.trigger_store.clone();
        let team_name = trigger.team_name.clone();
        let trigger_name = trigger.name.clone();
        let trigger_input = if trigger.input.is_empty() {
            format!("Triggered by incoming webhook at /{path}")
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
                Err(e) => {
                    error!(
                        trigger = %trigger_name,
                        team = %team_name,
                        %e,
                        "webhook-triggered team run failed"
                    );
                }
            }
            if let Err(e) = trigger_store.mark_fired(&trigger_name) {
                error!(trigger = %trigger_name, %e, "failed to mark webhook trigger as fired");
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
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

    #[tokio::test]
    async fn test_no_trigger_returns_404() {
        let state = make_state();
        let app = router(state);

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/github/pr")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
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

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/github/pr")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // 200 OK — the trigger matched (team run fails in test but we get 200)
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

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/secure/payload")
            .body(Body::empty())
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

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/gitlab/push")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
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

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/events")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_no_path_condition_matches_any() {
        let state = make_state();
        // Trigger with no path set should match any incoming path.
        state
            .trigger_store
            .create(
                "catch-all-hook",
                "webhook_received",
                r#"{}"#,
                "my-team",
                "",
            )
            .unwrap();

        let app = router(state);

        let req = Request::builder()
            .method("POST")
            .uri("/api/webhooks/anything/at/all")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
