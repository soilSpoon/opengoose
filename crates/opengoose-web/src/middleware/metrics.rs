use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::http::Request;
use axum::response::Response;
use tower::{Layer, Service};

use crate::metrics::RequestMetricsStore;

#[derive(Clone)]
pub(crate) struct MetricsLayer {
    metrics: RequestMetricsStore,
}

impl MetricsLayer {
    pub(crate) fn new(metrics: RequestMetricsStore) -> Self {
        Self { metrics }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

#[derive(Clone)]
pub struct MetricsService<S> {
    inner: S,
    metrics: RequestMetricsStore,
}

impl<S, B> Service<Request<B>> for MetricsService<S>
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
        let started = Instant::now();
        let metrics = self.metrics.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let response = inner.call(req).await?;
            if path != "/api/metrics" {
                metrics.record(response.status(), started.elapsed());
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn records_status_codes_for_non_metrics_routes() {
        let metrics = RequestMetricsStore::new();
        let app = Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }))
            .layer(MetricsLayer::new(metrics.clone()));

        let ok = app
            .clone()
            .oneshot(Request::builder().uri("/ok").body(Body::empty()).unwrap())
            .await
            .expect("request should succeed");
        assert_eq!(ok.status(), StatusCode::OK);

        let missing = app
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.request_count, 2);
        assert_eq!(snapshot.error_counts.len(), 1);
        assert_eq!(snapshot.error_counts[0].kind, "not_found");
    }

    #[tokio::test]
    async fn skips_metrics_scrape_route() {
        let metrics = RequestMetricsStore::new();
        let app = Router::new()
            .route("/api/metrics", get(|| async { "metrics" }))
            .layer(MetricsLayer::new(metrics.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(metrics.snapshot().request_count, 0);
    }
}
