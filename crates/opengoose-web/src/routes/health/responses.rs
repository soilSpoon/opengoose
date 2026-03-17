use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use opengoose_persistence::Database;
use opengoose_types::{ChannelMetricsStore, HealthStatus, ServiceProbeResponse};

use crate::data::{HealthResponse, probe_health, probe_readiness};

use super::snapshot::{MetricsSnapshot, load_metrics_snapshot};

#[derive(serde::Serialize)]
pub(crate) struct SessionMetrics {
    pub(crate) total: i64,
    pub(crate) messages: i64,
}

#[derive(serde::Serialize)]
pub(crate) struct QueueMetrics {
    pub(crate) pending: i64,
    pub(crate) processing: i64,
    pub(crate) completed: i64,
    pub(crate) failed: i64,
    pub(crate) dead: i64,
}

#[derive(serde::Serialize)]
pub(crate) struct RunMetrics {
    pub(crate) running: i64,
    pub(crate) completed: i64,
    pub(crate) failed: i64,
    pub(crate) suspended: i64,
}

#[derive(serde::Serialize)]
pub(crate) struct MetricsResponse {
    pub(crate) sessions: SessionMetrics,
    pub(crate) queue: QueueMetrics,
    pub(crate) runs: RunMetrics,
}

pub(super) fn build_health_response(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> anyhow::Result<HealthResponse> {
    probe_health(db, channel_metrics)
}

pub(super) fn build_ready_response(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> anyhow::Result<(HealthResponse, bool)> {
    probe_readiness(db, channel_metrics)
}

pub(super) fn build_live_response() -> ServiceProbeResponse {
    ServiceProbeResponse {
        status: HealthStatus::Healthy,
        checked_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    }
}

pub(super) fn build_metrics_response(db: Arc<Database>) -> anyhow::Result<MetricsResponse> {
    Ok(load_metrics_snapshot(db)?.into())
}

impl From<MetricsSnapshot> for MetricsResponse {
    fn from(snapshot: MetricsSnapshot) -> Self {
        Self {
            sessions: SessionMetrics {
                total: snapshot.session_count,
                messages: snapshot.message_count,
            },
            queue: QueueMetrics {
                pending: snapshot.queue_pending,
                processing: snapshot.queue_processing,
                completed: snapshot.queue_completed,
                failed: snapshot.queue_failed,
                dead: snapshot.queue_dead,
            },
            runs: RunMetrics {
                running: snapshot.running_runs,
                completed: snapshot.completed_runs,
                failed: snapshot.failed_runs,
                suspended: snapshot.suspended_runs,
            },
        }
    }
}
