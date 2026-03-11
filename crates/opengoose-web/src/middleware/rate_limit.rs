use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service};

/// Configuration for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed within the window.
    pub max_requests: u64,
    /// Sliding window duration.
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
        }
    }
}

/// Shared state tracking request timestamps per IP.
type CounterMap = Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>;

/// A [`tower::Layer`] that applies per-IP sliding window rate limiting.
#[derive(Clone)]
pub struct RateLimitLayer {
    config: RateLimitConfig,
    counters: CounterMap,
}

impl RateLimitLayer {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            counters: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            config: self.config.clone(),
            counters: self.counters.clone(),
        }
    }
}

/// The middleware service produced by [`RateLimitLayer`].
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    config: RateLimitConfig,
    counters: CounterMap,
}

impl<S, B> Service<Request<B>> for RateLimitService<S>
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
        let ip = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));

        let now = Instant::now();
        let window = self.config.window;
        let max = self.config.max_requests;

        let (remaining, retry_after) = {
            let mut map = self.counters.lock().unwrap_or_else(|e| e.into_inner());
            let entries = map.entry(ip).or_default();

            // Evict timestamps outside the window.
            entries.retain(|&t| now.duration_since(t) < window);

            if entries.len() as u64 >= max {
                // Calculate when the oldest entry in the window expires.
                let oldest = entries[0];
                let wait = window
                    .checked_sub(now.duration_since(oldest))
                    .unwrap_or(Duration::ZERO);
                (0u64, Some(wait.as_secs() + 1))
            } else {
                entries.push(now);
                let rem = max - entries.len() as u64;
                (rem, None)
            }
        };

        if let Some(retry_secs) = retry_after {
            return Box::pin(async move {
                let mut resp =
                    (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
                let headers = resp.headers_mut();
                headers.insert("X-RateLimit-Limit", max.into());
                headers.insert("X-RateLimit-Remaining", 0u64.into());
                headers.insert("Retry-After", retry_secs.into());
                Ok(resp)
            });
        }

        let mut inner = self.inner.clone();
        let limit = max;
        Box::pin(async move {
            let mut resp = inner.call(req).await?;
            let headers = resp.headers_mut();
            headers.insert("X-RateLimit-Limit", limit.into());
            headers.insert("X-RateLimit-Remaining", remaining.into());
            Ok(resp)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn test_app(max_requests: u64, window_secs: u64) -> Router {
        let config = RateLimitConfig {
            max_requests,
            window: Duration::from_secs(window_secs),
        };
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(RateLimitLayer::new(config))
    }

    fn make_request(addr: SocketAddr) -> Request<Body> {
        let mut req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    #[tokio::test]
    async fn allows_requests_under_limit() {
        let app = test_app(5, 60);
        let addr: SocketAddr = "10.0.0.1:1234".parse().unwrap();

        for i in 0..5 {
            let resp = app.clone().oneshot(make_request(addr)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "request {} should pass", i);
            assert_eq!(
                resp.headers()
                    .get("X-RateLimit-Remaining")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                (4 - i).to_string()
            );
            assert_eq!(
                resp.headers()
                    .get("X-RateLimit-Limit")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "5"
            );
        }
    }

    #[tokio::test]
    async fn blocks_when_limit_exceeded() {
        let app = test_app(2, 60);
        let addr: SocketAddr = "10.0.0.2:5678".parse().unwrap();

        // Two requests should pass.
        for _ in 0..2 {
            let resp = app.clone().oneshot(make_request(addr)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // Third request should be rejected.
        let resp = app.clone().oneshot(make_request(addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            resp.headers()
                .get("X-RateLimit-Remaining")
                .unwrap()
                .to_str()
                .unwrap(),
            "0"
        );
        assert!(resp.headers().get("Retry-After").is_some());
    }

    #[tokio::test]
    async fn zero_window_never_blocks_requests() {
        let app = test_app(1, 0);
        let addr: SocketAddr = "10.0.0.6:1111".parse().unwrap();

        for _ in 0..3 {
            let resp = app.clone().oneshot(make_request(addr)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(
                resp.headers()
                    .get("X-RateLimit-Remaining")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "0"
            );
        }
    }

    #[tokio::test]
    async fn separate_limits_per_ip() {
        let app = test_app(1, 60);
        let addr_a: SocketAddr = "10.0.0.3:1000".parse().unwrap();
        let addr_b: SocketAddr = "10.0.0.4:1000".parse().unwrap();

        let resp = app.clone().oneshot(make_request(addr_a)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app.clone().oneshot(make_request(addr_b)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // addr_a should now be blocked.
        let resp = app.clone().oneshot(make_request(addr_a)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // addr_b should also be blocked.
        let resp = app.clone().oneshot(make_request(addr_b)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn retry_after_header_present() {
        let app = test_app(1, 120);
        let addr: SocketAddr = "10.0.0.5:9000".parse().unwrap();

        let _ = app.clone().oneshot(make_request(addr)).await.unwrap();
        let resp = app.clone().oneshot(make_request(addr)).await.unwrap();

        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry: u64 = resp
            .headers()
            .get("Retry-After")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(retry > 0 && retry <= 121);
    }

    #[test]
    fn expired_entries_are_evicted_before_enforcing_limit() {
        let config = RateLimitConfig {
            max_requests: 2,
            window: Duration::from_secs(10),
        };
        let limiter = SlidingWindowRateLimiter::new(config.clone());
        let start = Instant::now();

        assert_eq!(
            limiter.check_key_with_config("10.0.0.7", &config, start),
            (1, None)
        );
        assert_eq!(
            limiter.check_key_with_config("10.0.0.7", &config, start + Duration::from_secs(5)),
            (0, None)
        );

        assert_eq!(
            limiter.check_key_with_config("10.0.0.7", &config, start + Duration::from_secs(11)),
            (0, None)
        );
        assert_eq!(
            limiter.check_key_with_config("10.0.0.7", &config, start + Duration::from_secs(11)),
            (0, Some(5))
        );
    }

    #[tokio::test]
    async fn default_config_values() {
        let cfg = RateLimitConfig::default();
        assert_eq!(cfg.max_requests, 100);
        assert_eq!(cfg.window, Duration::from_secs(60));
    }
}
