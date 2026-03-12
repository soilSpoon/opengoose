use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode;

const LATENCY_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestMetricsStore(Arc<Mutex<RequestMetricsInner>>);

#[derive(Debug)]
struct RequestMetricsInner {
    latency_bucket_counts: Vec<u64>,
    latency_sum_secs: f64,
    request_count: u64,
    error_counts: BTreeMap<ErrorMetricKey, u64>,
}

impl Default for RequestMetricsInner {
    fn default() -> Self {
        Self {
            latency_bucket_counts: vec![0; LATENCY_BUCKETS.len()],
            latency_sum_secs: 0.0,
            request_count: 0,
            error_counts: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ErrorMetricKey {
    kind: &'static str,
    status: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RequestMetricsSnapshot {
    pub(crate) latency_bucket_counts: Vec<u64>,
    pub(crate) latency_sum_secs: f64,
    pub(crate) request_count: u64,
    pub(crate) error_counts: Vec<ErrorMetric>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ErrorMetric {
    pub(crate) kind: &'static str,
    pub(crate) status: u16,
    pub(crate) count: u64,
}

impl RequestMetricsStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&self, status: StatusCode, elapsed: Duration) {
        let mut inner = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed_secs = elapsed.as_secs_f64();
        inner.request_count += 1;
        inner.latency_sum_secs += elapsed_secs;

        if let Some(index) = LATENCY_BUCKETS
            .iter()
            .position(|bucket| elapsed_secs <= *bucket)
        {
            inner.latency_bucket_counts[index] += 1;
        }

        if let Some(kind) = classify_error(status) {
            let key = ErrorMetricKey {
                kind,
                status: status.as_u16(),
            };
            *inner.error_counts.entry(key).or_insert(0) += 1;
        }
    }

    pub(crate) fn snapshot(&self) -> RequestMetricsSnapshot {
        let inner = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        RequestMetricsSnapshot {
            latency_bucket_counts: inner.latency_bucket_counts.clone(),
            latency_sum_secs: inner.latency_sum_secs,
            request_count: inner.request_count,
            error_counts: inner
                .error_counts
                .iter()
                .map(|(key, count)| ErrorMetric {
                    kind: key.kind,
                    status: key.status,
                    count: *count,
                })
                .collect(),
        }
    }

    pub(crate) fn latency_buckets() -> &'static [f64] {
        &LATENCY_BUCKETS
    }
}

fn classify_error(status: StatusCode) -> Option<&'static str> {
    match status {
        StatusCode::BAD_REQUEST => Some("bad_request"),
        StatusCode::UNAUTHORIZED => Some("unauthorized"),
        StatusCode::NOT_FOUND => Some("not_found"),
        StatusCode::CONFLICT => Some("conflict"),
        StatusCode::UNPROCESSABLE_ENTITY => Some("unprocessable_entity"),
        StatusCode::TOO_MANY_REQUESTS => Some("rate_limited"),
        _ if status.is_client_error() => Some("client_error"),
        _ if status.is_server_error() => Some("server_error"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_latency_buckets_and_totals() {
        let store = RequestMetricsStore::new();
        store.record(StatusCode::OK, Duration::from_millis(12));
        store.record(StatusCode::OK, Duration::from_secs(3));

        let snapshot = store.snapshot();

        assert_eq!(snapshot.request_count, 2);
        assert_eq!(snapshot.latency_bucket_counts[2], 1);
        assert_eq!(snapshot.latency_bucket_counts[9], 1);
        assert!(snapshot.latency_sum_secs >= 3.012);
    }

    #[test]
    fn records_error_counts_by_type_and_status() {
        let store = RequestMetricsStore::new();
        store.record(StatusCode::NOT_FOUND, Duration::from_millis(1));
        store.record(StatusCode::TOO_MANY_REQUESTS, Duration::from_millis(2));
        store.record(StatusCode::INTERNAL_SERVER_ERROR, Duration::from_millis(3));

        let snapshot = store.snapshot();

        assert_eq!(
            snapshot.error_counts,
            vec![
                ErrorMetric {
                    kind: "not_found",
                    status: 404,
                    count: 1,
                },
                ErrorMetric {
                    kind: "rate_limited",
                    status: 429,
                    count: 1,
                },
                ErrorMetric {
                    kind: "server_error",
                    status: 500,
                    count: 1,
                },
            ]
        );
    }
}
