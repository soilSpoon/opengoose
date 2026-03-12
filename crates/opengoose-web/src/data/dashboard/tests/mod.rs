use std::sync::Arc;

use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, HistoryMessage, MessageQueue,
    MessageType, OrchestrationRun, OrchestrationStore, QueueStats, RunStatus, SessionItem,
    SessionStore,
};
use opengoose_types::{Platform, SessionKey};

use super::activity::{activity_meta, build_dashboard_activities, synthetic_dashboard_activities};
use super::load_dashboard;
use super::metrics::{build_duration_bars, build_status_segments, duration_stats};
use crate::data::sessions::SessionRecord;

mod activity;
mod queue;
mod summary;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("db should open"))
}

fn empty_queue_stats() -> QueueStats {
    QueueStats {
        pending: 0,
        processing: 0,
        completed: 0,
        failed: 0,
        dead: 0,
    }
}

fn sample_run(
    team_run_id: &str,
    status: RunStatus,
    created_at: &str,
    updated_at: &str,
) -> OrchestrationRun {
    OrchestrationRun {
        team_run_id: team_run_id.into(),
        session_key: "discord:test:ops".into(),
        team_name: format!("team-{team_run_id}"),
        workflow: "chain".into(),
        input: "Investigate the live dashboard state".into(),
        status,
        current_step: 1,
        total_steps: 3,
        result: None,
        created_at: created_at.into(),
        updated_at: updated_at.into(),
    }
}

fn sample_session(
    session_key: &str,
    active_team: Option<&str>,
    role: &str,
    content: &str,
) -> SessionRecord {
    SessionRecord {
        summary: SessionItem {
            session_key: session_key.into(),
            active_team: active_team.map(str::to_string),
            selected_model: None,
            created_at: "2026-03-10 10:00".into(),
            updated_at: "2026-03-10 10:15".into(),
        },
        messages: vec![HistoryMessage {
            role: role.into(),
            content: content.into(),
            author: Some("tester".into()),
            created_at: "2026-03-10 10:15".into(),
        }],
    }
}
