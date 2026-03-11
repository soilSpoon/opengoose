use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, MessageQueue, MessageStatus, MessageType, OrchestrationRun, OrchestrationStore,
    QueueMessage, RunStatus, WorkItem, WorkItemStore, WorkStatus,
};

use super::selection::find_selected_run;

#[derive(Clone, Copy)]
pub(in crate::data) struct RunDataMode {
    pub(super) label: &'static str,
    pub(super) tone: &'static str,
    pub(super) using_mock: bool,
}

pub(in crate::data) struct LoadedRuns {
    pub(super) mode: RunDataMode,
    pub(super) runs: Vec<OrchestrationRun>,
}

pub(in crate::data) struct RunDetailRecord {
    pub(super) run: OrchestrationRun,
    pub(super) work_items: Vec<WorkItem>,
    pub(super) broadcasts: Vec<QueueMessage>,
}

const LIVE_RUNTIME_MODE: RunDataMode = RunDataMode {
    label: "Live runtime",
    tone: "success",
    using_mock: false,
};

const MOCK_PREVIEW_MODE: RunDataMode = RunDataMode {
    label: "Mock preview",
    tone: "neutral",
    using_mock: true,
};

pub(in crate::data) fn load_run_records(db: Arc<Database>, limit: i64) -> Result<LoadedRuns> {
    let runs = OrchestrationStore::new(db).list_runs(None, limit)?;

    if runs.is_empty() {
        Ok(LoadedRuns {
            mode: MOCK_PREVIEW_MODE,
            runs: mock_runs(),
        })
    } else {
        Ok(LoadedRuns {
            mode: LIVE_RUNTIME_MODE,
            runs,
        })
    }
}

pub(in crate::data) fn load_run_detail(
    db: Arc<Database>,
    loaded: &LoadedRuns,
    run_id: &str,
) -> Result<RunDetailRecord> {
    if loaded.mode.using_mock {
        Ok(mock_run_detail(find_selected_run(&loaded.runs, run_id)?))
    } else {
        load_live_run_detail(db, run_id)
    }
}

pub(in crate::data) fn mock_runs() -> Vec<OrchestrationRun> {
    vec![
        OrchestrationRun {
            team_run_id: "run-preview-01".into(),
            session_key: "discord:ns:studio-a:ops-bridge".into(),
            team_name: "feature-dev".into(),
            workflow: "chain".into(),
            input: "Implement the live dashboard shell and verify the orchestration views.".into(),
            status: RunStatus::Running,
            current_step: 2,
            total_steps: 4,
            result: None,
            created_at: "2026-03-10 10:02".into(),
            updated_at: "2026-03-10 10:29".into(),
        },
        OrchestrationRun {
            team_run_id: "run-preview-02".into(),
            session_key: "discord:ns:studio-a:ops-bridge".into(),
            team_name: "research-panel".into(),
            workflow: "fan_out".into(),
            input: "Compare provider latency across three channels.".into(),
            status: RunStatus::Completed,
            current_step: 3,
            total_steps: 3,
            result: Some(
                "Discord remains fastest for burst replies; Telegram is most stable under edit throttling."
                    .into(),
            ),
            created_at: "2026-03-10 08:15".into(),
            updated_at: "2026-03-10 08:33".into(),
        },
        OrchestrationRun {
            team_run_id: "run-preview-03".into(),
            session_key: "telegram:direct:founder-42".into(),
            team_name: "smart-router".into(),
            workflow: "router".into(),
            input: "Route an incoming request to the correct specialist.".into(),
            status: RunStatus::Suspended,
            current_step: 1,
            total_steps: 2,
            result: Some("Waiting on an external credential refresh before resuming.".into()),
            created_at: "2026-03-10 07:58".into(),
            updated_at: "2026-03-10 08:05".into(),
        },
    ]
}

fn load_live_run_detail(db: Arc<Database>, run_id: &str) -> Result<RunDetailRecord> {
    let run_store = OrchestrationStore::new(db.clone());
    let work_store = WorkItemStore::new(db.clone());
    let queue = MessageQueue::new(db);

    let run = run_store
        .get_run(run_id)?
        .with_context(|| format!("run `{run_id}` not found"))?;
    let work_items = work_store.list_for_run(run_id, None)?;
    let broadcasts = queue.read_broadcasts(run_id, None)?;

    Ok(RunDetailRecord {
        run,
        work_items,
        broadcasts,
    })
}

fn mock_run_detail(run: &OrchestrationRun) -> RunDetailRecord {
    let work_items = vec![
        WorkItem {
            id: 1,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            parent_id: None,
            title: "Frame the dashboard information architecture".into(),
            description: None,
            status: WorkStatus::Completed,
            assigned_to: Some("architect".into()),
            workflow_step: Some(0),
            input: None,
            output: None,
            error: None,
            created_at: run.created_at.clone(),
            updated_at: run.updated_at.clone(),
        },
        WorkItem {
            id: 2,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            parent_id: Some(1),
            title: "Implement Askama shell and HTMX detail panes".into(),
            description: None,
            status: WorkStatus::InProgress,
            assigned_to: Some("developer".into()),
            workflow_step: Some(1),
            input: None,
            output: None,
            error: None,
            created_at: run.created_at.clone(),
            updated_at: run.updated_at.clone(),
        },
    ];
    let broadcasts = vec![QueueMessage {
        id: 11,
        session_key: run.session_key.clone(),
        team_run_id: run.team_run_id.clone(),
        sender: "architect".into(),
        recipient: "broadcast".into(),
        content:
            "Signal-first layout approved. Proceed with the operations board visual direction."
                .into(),
        msg_type: MessageType::Broadcast,
        status: MessageStatus::Completed,
        retry_count: 0,
        max_retries: 3,
        created_at: run.updated_at.clone(),
        processed_at: None,
        error: None,
    }];

    RunDetailRecord {
        run: run.clone(),
        work_items,
        broadcasts,
    }
}
