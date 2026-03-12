use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service};

use opengoose_persistence::ApiKeyStore;

/// Paths that skip authentication.
const PUBLIC_PATHS: &[&str] = &[
    "/api/health",
    "/api/health/ready",
    "/api/health/live",
    "/api/metrics",
    "/api/openapi.json",
    "/api/docs",
];

/// A [`tower::Layer`] that enforces Bearer token authentication on API routes.
#[derive(Clone)]
pub struct AuthLayer {
    store: Arc<ApiKeyStore>,
}

impl AuthLayer {
    pub fn new(store: Arc<ApiKeyStore>) -> Self {
        Self { store }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            store: self.store.clone(),
        }
    }
}

/// The middleware service produced by [`AuthLayer`].
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    store: Arc<ApiKeyStore>,
}

impl<S, B> Service<Request<B>> for AuthService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let path = req.uri().path().to_string();

        // Skip auth for public endpoints.
        if PUBLIC_PATHS.iter().any(|p| path == *p) {
            let mut inner = self.inner.clone();
            return Box::pin(async move { inner.call(req).await });
        }

        // Extract Bearer token from Authorization header.
        let token = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t.to_string());

        let store = self.store.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let Some(token) = token else {
                return Ok(unauthorized("missing Authorization header"));
            };

            match store.validate(&token) {
                Ok(true) => inner.call(req).await,
                Ok(false) => Ok(unauthorized("invalid API key")),
                Err(_) => Ok(
                    (StatusCode::INTERNAL_SERVER_ERROR, "auth validation error").into_response()
                ),
            }
        })
    }
}

fn unauthorized(message: &str) -> Response {
    (StatusCode::UNAUTHORIZED, message.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderValue, Request};
    use axum::routing::get;
    use opengoose_persistence::Database;
    use tower::ServiceExt;

    fn test_app() -> (Router, Arc<ApiKeyStore>) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opengoose-auth-test-{}-{suffix}.db",
            std::process::id()
        ));
        let db = Arc::new(Database::open_at(path).unwrap());
        let store = Arc::new(ApiKeyStore::new(db));
        let app = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .route("/api/health", get(|| async { "healthy" }))
            .route("/api/health/ready", get(|| async { "ready" }))
            .route("/api/health/live", get(|| async { "live" }))
            .route("/api/metrics", get(|| async { "metrics" }))
            .route("/api/healthcheck", get(|| async { "protected" }))
            .route("/api/openapi.json", get(|| async { "{}" }))
            .route("/api/docs", get(|| async { "docs" }))
            .layer(AuthLayer::new(store.clone()));
        (app, store)
    }

    #[tokio::test]
    async fn public_endpoints_skip_auth() {
        let (app, _store) = test_app();

        for path in PUBLIC_PATHS {
            let req = Request::builder().uri(*path).body(Body::empty()).unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "public path {path} should not require auth"
            );
        }
    }

    #[tokio::test]
    async fn bypass_like_public_paths_still_require_auth() {
        let (app, _store) = test_app();

        for path in ["/api/healthcheck"] {
            let req = Request::builder().uri(path).body(Body::empty()).unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "{path} should not bypass auth",
            );
        }
    }

    #[tokio::test]
    async fn protected_endpoint_rejects_without_token() {
        let (app, _store) = test_app();
        let req = Request::builder()
            .uri("/api/test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_endpoint_rejects_invalid_token() {
        let (app, _store) = test_app();
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", "Bearer ogk_invalid")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn protected_endpoint_accepts_valid_token() {
        let (app, store) = test_app();
        let key = store.generate(Some("test")).unwrap();
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", format!("Bearer {}", key.plaintext))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_non_bearer_auth() {
        let (app, _store) = test_app();
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_authorization_header_with_invalid_bytes() {
        let (app, _store) = test_app();
        let req = Request::builder()
            .uri("/api/test")
            .header(
                "authorization",
                HeaderValue::from_bytes(b"Bearer \xFFtoken").unwrap(),
            )
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoked_key_is_rejected() {
        let (app, store) = test_app();
        let key = store.generate(Some("temp")).unwrap();

        // Key works before revocation
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", format!("Bearer {}", key.plaintext))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Revoke
        store.revoke(&key.id).unwrap();

        // Key rejected after revocation
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", format!("Bearer {}", key.plaintext))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_empty_bearer_token() {
        let (app, _store) = test_app();
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", "Bearer ")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_bearer_prefix_only() {
        let (app, _store) = test_app();
        // "Bearer" without the trailing space — strip_prefix("Bearer ") returns None
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", "Bearer")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn public_paths_exact_match_only() {
        let (app, _store) = test_app();
        // Subpaths of public endpoints should still require auth
        // (not registered in our test router, so 404, but the auth layer
        // should not skip them — we verify it doesn't return 401 bypass)
        let req = Request::builder()
            .uri("/api/health/ready/extra")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Without auth, should get 401 (auth layer blocks before routing)
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn multiple_keys_independently_valid() {
        let (app, store) = test_app();
        let key1 = store.generate(Some("first")).unwrap();
        let key2 = store.generate(Some("second")).unwrap();

        // Both keys work
        for key in [&key1, &key2] {
            let req = Request::builder()
                .uri("/api/test")
                .header("authorization", format!("Bearer {}", key.plaintext))
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // Revoke first key — second still works
        store.revoke(&key1.id).unwrap();
        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", format!("Bearer {}", key2.plaintext))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let req = Request::builder()
            .uri("/api/test")
            .header("authorization", format!("Bearer {}", key1.plaintext))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn unauthorized_response_has_correct_status() {
        let resp = unauthorized("test error");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn auth_layer_produces_auth_service() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opengoose-auth-layer-test-{}-{suffix}.db",
            std::process::id()
        ));
        let db = Arc::new(Database::open_at(path).unwrap());
        let store = Arc::new(ApiKeyStore::new(db));
        let layer = AuthLayer::new(store);
        // Verify the layer can wrap a service (type-level check)
        let _service: AuthService<tower::util::ServiceFn<fn(Request<Body>) -> _>> =
            layer.layer(tower::service_fn(|_req: Request<Body>| async {
                Ok::<_, std::convert::Infallible>(StatusCode::OK.into_response())
            }));
    }
}
