use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use opengoose_persistence::{Database, MessageQueue, OrchestrationStore, SessionStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey};
use tracing::warn;

pub(crate) const LIVE_EVENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueueSnapshot {
    last_message_id: Option<i32>,
    last_team_run_id: Option<String>,
    pending: i64,
    processing: i64,
    completed: i64,
    failed: i64,
    dead: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveSnapshot {
    sessions: HashMap<String, String>,
    runs: HashMap<String, (String, String)>,
    queue: QueueSnapshot,
}

pub(crate) fn capture_live_snapshot(db: Arc<Database>) -> anyhow::Result<LiveSnapshot> {
    let session_store = SessionStore::new(db.clone());
    let orchestration_store = OrchestrationStore::new(db.clone());
    let queue_store = MessageQueue::new(db);

    let sessions = session_store
        .list_sessions(256)?
        .into_iter()
        .map(|session| (session.session_key, session.updated_at))
        .collect();

    let runs = orchestration_store
        .list_runs(None, 256)?
        .into_iter()
        .map(|run| {
            (
                run.team_run_id,
                (run.updated_at, run.status.as_str().to_string()),
            )
        })
        .collect();

    let queue_stats = queue_store.stats()?;
    let recent_queue = queue_store.list_recent(1)?;
    let queue = QueueSnapshot {
        last_message_id: recent_queue.first().map(|message| message.id),
        last_team_run_id: recent_queue
            .first()
            .map(|message| message.team_run_id.clone()),
        pending: queue_stats.pending,
        processing: queue_stats.processing,
        completed: queue_stats.completed,
        failed: queue_stats.failed,
        dead: queue_stats.dead,
    };

    Ok(LiveSnapshot {
        sessions,
        runs,
        queue,
    })
}

fn emit_live_snapshot_changes(
    previous: &LiveSnapshot,
    current: &LiveSnapshot,
    event_bus: &EventBus,
) {
    let mut dashboard_changed = false;

    for (session_key, updated_at) in &current.sessions {
        if previous.sessions.get(session_key) != Some(updated_at) {
            dashboard_changed = true;
            event_bus.emit(AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id(session_key),
            });
        }
    }
    if previous.sessions.len() != current.sessions.len() {
        dashboard_changed = true;
    }

    for (team_run_id, state) in &current.runs {
        if previous.runs.get(team_run_id) != Some(state) {
            dashboard_changed = true;
            event_bus.emit(AppEventKind::RunUpdated {
                team_run_id: team_run_id.clone(),
                status: state.1.clone(),
            });
        }
    }
    if previous.runs.len() != current.runs.len() {
        dashboard_changed = true;
    }

    if previous.queue != current.queue {
        dashboard_changed = true;
        event_bus.emit(AppEventKind::QueueUpdated {
            team_run_id: current.queue.last_team_run_id.clone(),
        });
    }

    if dashboard_changed {
        event_bus.emit(AppEventKind::DashboardUpdated);
    }
}

pub(crate) fn spawn_live_event_watcher(db: Arc<Database>, event_bus: EventBus) {
    tokio::spawn(async move {
        let mut snapshot = match capture_live_snapshot(db.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!(%error, "failed to capture initial live snapshot");
                LiveSnapshot::default()
            }
        };

        let mut ticker = tokio::time::interval(LIVE_EVENT_POLL_INTERVAL);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match capture_live_snapshot(db.clone()) {
                Ok(next) => {
                    emit_live_snapshot_changes(&snapshot, &next, &event_bus);
                    snapshot = next;
                }
                Err(error) => warn!(%error, "failed to refresh live snapshot"),
            }
        }
    });
}
