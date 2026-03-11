use std::collections::HashMap;
use std::sync::Arc;

use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, SessionStore};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct QueueSnapshot {
    pub(super) last_message_id: Option<i32>,
    pub(super) last_team_run_id: Option<String>,
    pub(super) pending: i64,
    pub(super) processing: i64,
    pub(super) completed: i64,
    pub(super) failed: i64,
    pub(super) dead: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LiveSnapshot {
    pub(super) sessions: HashMap<String, String>,
    pub(super) runs: HashMap<String, (String, String)>,
    pub(super) queue: QueueSnapshot,
}

pub(super) fn capture_live_snapshot(db: Arc<Database>) -> anyhow::Result<LiveSnapshot> {
    Ok(LiveSnapshot {
        sessions: capture_sessions(db.clone())?,
        runs: capture_runs(db.clone())?,
        queue: capture_queue_snapshot(db)?,
    })
}

fn capture_sessions(db: Arc<Database>) -> anyhow::Result<HashMap<String, String>> {
    let session_store = SessionStore::new(db);
    Ok(session_store
        .list_sessions(256)?
        .into_iter()
        .map(|session| (session.session_key, session.updated_at))
        .collect())
}

fn capture_runs(db: Arc<Database>) -> anyhow::Result<HashMap<String, (String, String)>> {
    let orchestration_store = OrchestrationStore::new(db);
    Ok(orchestration_store
        .list_runs(None, 256)?
        .into_iter()
        .map(|run| {
            (
                run.team_run_id,
                (run.updated_at, run.status.as_str().to_string()),
            )
        })
        .collect())
}

fn capture_queue_snapshot(db: Arc<Database>) -> anyhow::Result<QueueSnapshot> {
    let queue_store = MessageQueue::new(db);
    let queue_stats = queue_store.stats()?;
    let recent_queue = queue_store.list_recent(1)?;

    Ok(QueueSnapshot {
        last_message_id: recent_queue.first().map(|message| message.id),
        last_team_run_id: recent_queue
            .first()
            .map(|message| message.team_run_id.clone()),
        pending: queue_stats.pending,
        processing: queue_stats.processing,
        completed: queue_stats.completed,
        failed: queue_stats.failed,
        dead: queue_stats.dead,
    })
}
