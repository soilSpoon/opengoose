use std::sync::Arc;

use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, SessionStore};

#[derive(Debug)]
pub(super) struct MetricsSnapshot {
    pub(super) session_count: i64,
    pub(super) message_count: i64,
    pub(super) queue_pending: i64,
    pub(super) queue_processing: i64,
    pub(super) queue_completed: i64,
    pub(super) queue_failed: i64,
    pub(super) queue_dead: i64,
    pub(super) running_runs: i64,
    pub(super) completed_runs: i64,
    pub(super) failed_runs: i64,
    pub(super) suspended_runs: i64,
}

pub(super) fn load_metrics_snapshot(db: Arc<Database>) -> anyhow::Result<MetricsSnapshot> {
    let session_stats = SessionStore::new(db.clone()).stats()?;
    let queue_stats = MessageQueue::new(db.clone()).stats()?;
    let run_counts = OrchestrationStore::new(db).count_runs_by_status()?;

    Ok(MetricsSnapshot {
        session_count: session_stats.session_count,
        message_count: session_stats.message_count,
        queue_pending: queue_stats.pending,
        queue_processing: queue_stats.processing,
        queue_completed: queue_stats.completed,
        queue_failed: queue_stats.failed,
        queue_dead: queue_stats.dead,
        running_runs: run_counts.running,
        completed_runs: run_counts.completed,
        failed_runs: run_counts.failed,
        suspended_runs: run_counts.suspended,
    })
}
