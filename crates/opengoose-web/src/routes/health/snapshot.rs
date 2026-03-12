use std::sync::Arc;

use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, RunStatus, SessionStore};

#[derive(Debug)]
pub(super) struct MetricsSnapshot {
    pub(super) session_count: i64,
    pub(super) message_count: i64,
    pub(super) queue_pending: i64,
    pub(super) queue_processing: i64,
    pub(super) queue_completed: i64,
    pub(super) queue_failed: i64,
    pub(super) queue_dead: i64,
    pub(super) running_runs: usize,
    pub(super) completed_runs: usize,
    pub(super) failed_runs: usize,
    pub(super) suspended_runs: usize,
}

pub(super) fn load_metrics_snapshot(db: Arc<Database>) -> anyhow::Result<MetricsSnapshot> {
    let session_stats = SessionStore::new(db.clone()).stats()?;
    let queue_stats = MessageQueue::new(db.clone()).stats()?;
    let recent_runs = OrchestrationStore::new(db).list_runs(None, 200)?;

    let mut running_runs = 0;
    let mut completed_runs = 0;
    let mut failed_runs = 0;
    let mut suspended_runs = 0;

    for run in &recent_runs {
        match run.status {
            RunStatus::Running => running_runs += 1,
            RunStatus::Completed => completed_runs += 1,
            RunStatus::Failed => failed_runs += 1,
            RunStatus::Suspended => suspended_runs += 1,
        }
    }

    Ok(MetricsSnapshot {
        session_count: session_stats.session_count,
        message_count: session_stats.message_count,
        queue_pending: queue_stats.pending,
        queue_processing: queue_stats.processing,
        queue_completed: queue_stats.completed,
        queue_failed: queue_stats.failed,
        queue_dead: queue_stats.dead,
        running_runs,
        completed_runs,
        failed_runs,
        suspended_runs,
    })
}
