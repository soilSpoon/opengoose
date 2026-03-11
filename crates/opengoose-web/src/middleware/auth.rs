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
    use axum::http::Request;
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
            .route("/api/openapi.json", get(|| async { "{}" }))
            .route("/api/docs", get(|| async { "docs" }))
            .layer(AuthLayer::new(store.clone()));
        (app, store)
    }

    #[tokio::test]
    async fn public_endpoints_skip_auth() {
        let (app, _store) = test_app();

        for path in &["/api/health", "/api/openapi.json", "/api/docs"] {
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
}
