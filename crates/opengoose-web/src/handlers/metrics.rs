use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};

use opengoose_persistence::{MessageQueue, RunStatus};
use opengoose_types::ChannelMetricsSnapshot;

use super::AppError;
use crate::metrics::RequestMetricsSnapshot;
use crate::state::AppState;

const KNOWN_GATEWAY_PLATFORMS: &[&str] = &["discord", "slack", "telegram", "matrix"];

#[derive(Clone, Debug)]
struct PrometheusSnapshot {
    active_sessions: i64,
    total_messages: i64,
    queue_pending: i64,
    queue_processing: i64,
    queue_completed: i64,
    queue_failed: i64,
    queue_dead: i64,
    running_runs: usize,
    completed_runs: usize,
    failed_runs: usize,
    suspended_runs: usize,
    gateway_snapshots: HashMap<String, ChannelMetricsSnapshot>,
    request_metrics: RequestMetricsSnapshot,
}

#[derive(Clone, Copy, Debug, Default)]
struct GatewayStateCounts {
    connected: usize,
    reconnecting: usize,
    disconnected: usize,
}

/// GET /api/metrics — return Prometheus-compatible runtime metrics.
pub async fn get_metrics(State(state): State<AppState>) -> Result<Response, AppError> {
    let session_stats = state.session_store.stats()?;
    let queue_stats = MessageQueue::new(state.db.clone()).stats()?;
    let recent_runs = state.orchestration_store.list_runs(None, 200)?;

    let snapshot = PrometheusSnapshot {
        active_sessions: session_stats.session_count,
        total_messages: session_stats.message_count,
        queue_pending: queue_stats.pending,
        queue_processing: queue_stats.processing,
        queue_completed: queue_stats.completed,
        queue_failed: queue_stats.failed,
        queue_dead: queue_stats.dead,
        running_runs: recent_runs
            .iter()
            .filter(|run| run.status == RunStatus::Running)
            .count(),
        completed_runs: recent_runs
            .iter()
            .filter(|run| run.status == RunStatus::Completed)
            .count(),
        failed_runs: recent_runs
            .iter()
            .filter(|run| run.status == RunStatus::Failed)
            .count(),
        suspended_runs: recent_runs
            .iter()
            .filter(|run| run.status == RunStatus::Suspended)
            .count(),
        gateway_snapshots: state.channel_metrics.snapshot(),
        request_metrics: state.request_metrics.snapshot(),
    };

    Ok((
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        render_prometheus(&snapshot),
    )
        .into_response())
}

fn render_prometheus(snapshot: &PrometheusSnapshot) -> String {
    let mut output = String::new();

    emit_metric_header(
        &mut output,
        "opengoose_sessions_active",
        "gauge",
        "Current number of persisted conversation sessions.",
    );
    emit_value(
        &mut output,
        "opengoose_sessions_active",
        snapshot.active_sessions,
    );

    emit_metric_header(
        &mut output,
        "opengoose_messages_total",
        "gauge",
        "Current number of persisted messages across all sessions.",
    );
    emit_value(
        &mut output,
        "opengoose_messages_total",
        snapshot.total_messages,
    );

    emit_metric_header(
        &mut output,
        "opengoose_queue_messages",
        "gauge",
        "Current message queue entries grouped by status.",
    );
    emit_labeled_value(
        &mut output,
        "opengoose_queue_messages",
        &[("status", "pending")],
        snapshot.queue_pending,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_queue_messages",
        &[("status", "processing")],
        snapshot.queue_processing,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_queue_messages",
        &[("status", "completed")],
        snapshot.queue_completed,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_queue_messages",
        &[("status", "failed")],
        snapshot.queue_failed,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_queue_messages",
        &[("status", "dead")],
        snapshot.queue_dead,
    );

    emit_metric_header(
        &mut output,
        "opengoose_runs",
        "gauge",
        "Current orchestration runs grouped by status from the latest persisted snapshot.",
    );
    emit_labeled_value(
        &mut output,
        "opengoose_runs",
        &[("status", "running")],
        snapshot.running_runs,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_runs",
        &[("status", "completed")],
        snapshot.completed_runs,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_runs",
        &[("status", "failed")],
        snapshot.failed_runs,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_runs",
        &[("status", "suspended")],
        snapshot.suspended_runs,
    );

    let gateway_counts = gateway_state_counts(&snapshot.gateway_snapshots);
    emit_metric_header(
        &mut output,
        "opengoose_gateway_connections",
        "gauge",
        "Tracked gateway platforms grouped by connection state.",
    );
    emit_labeled_value(
        &mut output,
        "opengoose_gateway_connections",
        &[("state", "connected")],
        gateway_counts.connected,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_gateway_connections",
        &[("state", "reconnecting")],
        gateway_counts.reconnecting,
    );
    emit_labeled_value(
        &mut output,
        "opengoose_gateway_connections",
        &[("state", "disconnected")],
        gateway_counts.disconnected,
    );

    emit_metric_header(
        &mut output,
        "opengoose_gateway_connected",
        "gauge",
        "Whether a tracked gateway platform is currently connected (1) or not (0).",
    );
    emit_metric_header(
        &mut output,
        "opengoose_gateway_reconnects_total",
        "counter",
        "Total reconnect attempts recorded per gateway platform since startup.",
    );

    for platform in gateway_platforms(&snapshot.gateway_snapshots) {
        let gateway = snapshot.gateway_snapshots.get(&platform);
        let connected = gateway
            .and_then(|details| details.uptime_secs)
            .map(|_| 1u8)
            .unwrap_or(0u8);
        let reconnects = gateway.map(|details| details.reconnect_count).unwrap_or(0);

        emit_labeled_value(
            &mut output,
            "opengoose_gateway_connected",
            &[("platform", platform.as_str())],
            connected,
        );
        emit_labeled_value(
            &mut output,
            "opengoose_gateway_reconnects_total",
            &[("platform", platform.as_str())],
            reconnects,
        );
    }

    emit_metric_header(
        &mut output,
        "opengoose_http_request_duration_seconds",
        "histogram",
        "Observed HTTP API request latencies in seconds.",
    );
    let mut cumulative = 0u64;
    for (index, bucket) in crate::metrics::RequestMetricsStore::latency_buckets()
        .iter()
        .enumerate()
    {
        cumulative += snapshot.request_metrics.latency_bucket_counts[index];
        emit_labeled_value(
            &mut output,
            "opengoose_http_request_duration_seconds_bucket",
            &[("le", &bucket.to_string())],
            cumulative,
        );
    }
    emit_labeled_value(
        &mut output,
        "opengoose_http_request_duration_seconds_bucket",
        &[("le", "+Inf")],
        snapshot.request_metrics.request_count,
    );
    emit_float_value(
        &mut output,
        "opengoose_http_request_duration_seconds_sum",
        snapshot.request_metrics.latency_sum_secs,
    );
    emit_value(
        &mut output,
        "opengoose_http_request_duration_seconds_count",
        snapshot.request_metrics.request_count,
    );

    emit_metric_header(
        &mut output,
        "opengoose_http_errors_total",
        "counter",
        "Typed HTTP error counters. Prometheus rate() can derive error rates from these series.",
    );
    for error in &snapshot.request_metrics.error_counts {
        emit_labeled_value(
            &mut output,
            "opengoose_http_errors_total",
            &[("type", error.kind), ("status", &error.status.to_string())],
            error.count,
        );
    }

    output
}

fn gateway_platforms(snapshots: &HashMap<String, ChannelMetricsSnapshot>) -> BTreeSet<String> {
    let mut platforms = KNOWN_GATEWAY_PLATFORMS
        .iter()
        .map(|platform| (*platform).to_string())
        .collect::<BTreeSet<_>>();
    platforms.extend(snapshots.keys().cloned());
    platforms
}

fn gateway_state_counts(snapshots: &HashMap<String, ChannelMetricsSnapshot>) -> GatewayStateCounts {
    let mut counts = GatewayStateCounts::default();
    for platform in gateway_platforms(snapshots) {
        match snapshots.get(&platform) {
            Some(snapshot) if snapshot.uptime_secs.is_some() => counts.connected += 1,
            Some(snapshot) if snapshot.reconnect_count > 0 => counts.reconnecting += 1,
            _ => counts.disconnected += 1,
        }
    }
    counts
}

fn emit_metric_header(output: &mut String, name: &str, metric_type: &str, help: &str) {
    let _ = writeln!(output, "# HELP {name} {help}");
    let _ = writeln!(output, "# TYPE {name} {metric_type}");
}

fn emit_value(output: &mut String, name: &str, value: impl std::fmt::Display) {
    let _ = writeln!(output, "{name} {value}");
}

fn emit_float_value(output: &mut String, name: &str, value: f64) {
    let _ = writeln!(output, "{name} {value}");
}

fn emit_labeled_value(
    output: &mut String,
    name: &str,
    labels: &[(&str, &str)],
    value: impl std::fmt::Display,
) {
    let labels = labels
        .iter()
        .map(|(key, value)| format!(r#"{key}="{}""#, escape_label_value(value)))
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(output, "{name}{{{labels}}} {value}");
}

fn escape_label_value(value: &str) -> String {
    value.replace('\\', r"\\").replace('"', r#"\""#)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::to_bytes;
    use axum::extract::State;
    use axum::http::{StatusCode, header};
    use opengoose_persistence::MessageType;
    use opengoose_types::{Platform, SessionKey};

    use super::*;
    use crate::handlers::test_support::make_state;

    #[tokio::test]
    async fn metrics_handler_returns_prometheus_text() {
        let state = make_state();
        let session_key = SessionKey::new(Platform::Discord, "guild", "channel");
        state
            .session_store
            .append_user_message(&session_key, "hello", Some("alice"))
            .expect("user message should be stored");
        state
            .session_store
            .append_assistant_message(&session_key, "hi")
            .expect("assistant message should be stored");
        state.channel_metrics.set_connected("discord");
        state
            .channel_metrics
            .record_reconnect("slack", Some("timeout".into()));
        state
            .request_metrics
            .record(StatusCode::OK, Duration::from_millis(20));
        state
            .request_metrics
            .record(StatusCode::NOT_FOUND, Duration::from_millis(30));

        let response = get_metrics(State(state))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; version=0.0.4; charset=utf-8"
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        let text = String::from_utf8(body.to_vec()).expect("body should be utf-8");

        assert!(text.contains("# TYPE opengoose_sessions_active gauge"));
        assert!(text.contains("opengoose_sessions_active 1"));
        assert!(text.contains("opengoose_messages_total 2"));
        assert!(text.contains("opengoose_gateway_connections{state=\"connected\"} 1"));
        assert!(text.contains("opengoose_gateway_connections{state=\"reconnecting\"} 1"));
        assert!(text.contains("opengoose_gateway_connections{state=\"disconnected\"} 2"));
        assert!(text.contains("opengoose_http_request_duration_seconds_count 2"));
        assert!(text.contains("opengoose_http_errors_total{type=\"not_found\",status=\"404\"} 1"));
    }

    #[tokio::test]
    async fn metrics_handler_includes_run_and_queue_series() {
        let state = make_state();
        let queue = MessageQueue::new(Arc::clone(&state.db));
        state
            .orchestration_store
            .create_run("run-1", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("running run should be created");
        state
            .orchestration_store
            .create_run("run-2", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("completed run should be created");
        state
            .orchestration_store
            .complete_run("run-2", "done")
            .expect("run should complete");
        state
            .orchestration_store
            .create_run("run-3", "discord:ns:guild:chan", "ops", "chain", "input", 2)
            .expect("failed run should be created");
        state
            .orchestration_store
            .fail_run("run-3", "boom")
            .expect("run should fail");
        queue
            .enqueue(
                "discord:ns:guild:chan",
                "run-1",
                "user",
                "worker",
                "task",
                MessageType::Task,
            )
            .expect("queue item should be created");

        let response = get_metrics(State(state))
            .await
            .expect("handler should succeed");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        let text = String::from_utf8(body.to_vec()).expect("body should be utf-8");

        assert!(text.contains("opengoose_runs{status=\"running\"} 1"));
        assert!(text.contains("opengoose_runs{status=\"completed\"} 1"));
        assert!(text.contains("opengoose_runs{status=\"failed\"} 1"));
        assert!(text.contains("opengoose_queue_messages{status=\"pending\"} 1"));
    }
}
