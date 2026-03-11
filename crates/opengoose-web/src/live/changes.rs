use opengoose_types::{AppEventKind, EventBus, SessionKey};

use super::snapshot::LiveSnapshot;

pub(super) fn emit_live_snapshot_changes(
    previous: &LiveSnapshot,
    current: &LiveSnapshot,
    event_bus: &EventBus,
) {
    let sessions_changed = emit_session_changes(previous, current, event_bus);
    let runs_changed = emit_run_changes(previous, current, event_bus);
    let queue_changed = emit_queue_changes(previous, current, event_bus);
    let dashboard_changed = sessions_changed || runs_changed || queue_changed;

    if dashboard_changed {
        event_bus.emit(AppEventKind::DashboardUpdated);
    }
}

fn emit_session_changes(
    previous: &LiveSnapshot,
    current: &LiveSnapshot,
    event_bus: &EventBus,
) -> bool {
    let mut changed = false;

    for (session_key, updated_at) in &current.sessions {
        if previous.sessions.get(session_key) != Some(updated_at) {
            changed = true;
            event_bus.emit(AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id(session_key),
            });
        }
    }

    changed || previous.sessions.len() != current.sessions.len()
}

fn emit_run_changes(previous: &LiveSnapshot, current: &LiveSnapshot, event_bus: &EventBus) -> bool {
    let mut changed = false;

    for (team_run_id, state) in &current.runs {
        if previous.runs.get(team_run_id) != Some(state) {
            changed = true;
            event_bus.emit(AppEventKind::RunUpdated {
                team_run_id: team_run_id.clone(),
                status: state.1.clone(),
            });
        }
    }

    changed || previous.runs.len() != current.runs.len()
}

fn emit_queue_changes(
    previous: &LiveSnapshot,
    current: &LiveSnapshot,
    event_bus: &EventBus,
) -> bool {
    if previous.queue == current.queue {
        return false;
    }

    event_bus.emit(AppEventKind::QueueUpdated {
        team_run_id: current.queue.last_team_run_id.clone(),
    });
    true
}
