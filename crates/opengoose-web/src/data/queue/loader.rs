use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, MessageQueue, OrchestrationRun, OrchestrationStore, QueueMessage, QueueStats,
};

use crate::data::runs::mock_runs;

#[derive(Clone, Copy)]
pub(super) struct QueueDataMode {
    pub(super) label: &'static str,
    pub(super) tone: &'static str,
    using_mock: bool,
}

pub(super) struct LoadedQueueRuns {
    pub(super) mode: QueueDataMode,
    pub(super) runs: Vec<OrchestrationRun>,
}

pub(super) struct QueueDetailRecord {
    pub(super) run: OrchestrationRun,
    pub(super) messages: Vec<QueueMessage>,
    pub(super) dead_letters: Vec<QueueMessage>,
    pub(super) stats: QueueStats,
}

const LIVE_RUNTIME_MODE: QueueDataMode = QueueDataMode {
    label: "Live runtime",
    tone: "success",
    using_mock: false,
};

const MOCK_PREVIEW_MODE: QueueDataMode = QueueDataMode {
    label: "Mock preview",
    tone: "neutral",
    using_mock: true,
};

pub(super) fn load_queue_runs(db: Arc<Database>, limit: i64) -> Result<LoadedQueueRuns> {
    let runs = OrchestrationStore::new(db).list_runs(None, limit)?;

    if runs.is_empty() {
        Ok(LoadedQueueRuns {
            mode: MOCK_PREVIEW_MODE,
            runs: mock_runs(),
        })
    } else {
        Ok(LoadedQueueRuns {
            mode: LIVE_RUNTIME_MODE,
            runs,
        })
    }
}

pub(super) fn load_queue_detail(
    db: Arc<Database>,
    loaded: &LoadedQueueRuns,
    run_id: &str,
) -> Result<QueueDetailRecord> {
    if loaded.mode.using_mock {
        Ok(mock_queue_detail(find_selected_run(&loaded.runs, run_id)?))
    } else {
        load_live_queue_detail(db, run_id)
    }
}

fn load_live_queue_detail(db: Arc<Database>, run_id: &str) -> Result<QueueDetailRecord> {
    let run_store = OrchestrationStore::new(db.clone());
    let queue = MessageQueue::new(db);
    let run = run_store
        .get_run(run_id)?
        .with_context(|| format!("run `{run_id}` not found"))?;
    let messages = queue.list_for_run(run_id)?;
    let dead_letters = queue.get_dead_letters(run_id)?;
    let stats = queue.stats()?;

    Ok(QueueDetailRecord {
        run,
        messages,
        dead_letters,
        stats,
    })
}

pub(super) fn mock_queue_detail(run: &OrchestrationRun) -> QueueDetailRecord {
    let messages = vec![
        QueueMessage {
            id: 1,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            sender: "planner".into(),
            recipient: "developer".into(),
            content: "Implement the shell and wire the detail routes.".into(),
            msg_type: opengoose_persistence::MessageType::Task,
            status: opengoose_persistence::MessageStatus::Completed,
            retry_count: 0,
            max_retries: 3,
            created_at: "2026-03-10 10:10".into(),
            processed_at: None,
            error: None,
        },
        QueueMessage {
            id: 2,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            sender: "developer".into(),
            recipient: "reviewer".into(),
            content: "Review the CSS variables and the responsive breakpoint strategy.".into(),
            msg_type: opengoose_persistence::MessageType::Delegation,
            status: opengoose_persistence::MessageStatus::Pending,
            retry_count: 1,
            max_retries: 3,
            created_at: "2026-03-10 10:16".into(),
            processed_at: None,
            error: Some("waiting for reviewer pickup".into()),
        },
    ];
    let stats = QueueStats {
        pending: 1,
        processing: 0,
        completed: 1,
        failed: 0,
        dead: 0,
    };

    QueueDetailRecord {
        run: run.clone(),
        messages,
        dead_letters: Vec::new(),
        stats,
    }
}

fn find_selected_run<'a>(
    runs: &'a [OrchestrationRun],
    run_id: &str,
) -> Result<&'a OrchestrationRun> {
    runs.iter()
        .find(|run| run.team_run_id == run_id)
        .with_context(|| format!("selected queue run `{run_id}` missing"))
}
