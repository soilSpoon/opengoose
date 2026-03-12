use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, MessageQueue, MessageStatus, MessageType, OrchestrationRun, OrchestrationStore,
    QueueMessage, RunStatus, WorkItem, WorkItemStore, WorkStatus,
};
use urlencoding::encode;

use crate::data::utils::{choose_selected_run, progress_label, run_tone, work_tone};
use crate::data::views::{
    BroadcastView, MetaRow, RunDetailView, RunListItem, RunsPageView, WorkItemView,
};

/// Load the runs page view-model, optionally selecting a run by ID.
pub fn load_runs_page(db: Arc<Database>, selected: Option<String>) -> Result<RunsPageView> {
    let run_store = OrchestrationStore::new(db.clone());
    let runs = run_store.list_runs(None, 20)?;
    let using_mock = runs.is_empty();

    let selected_run_id = if using_mock {
        choose_selected_run(&mock_runs(), selected)
    } else {
        choose_selected_run(&runs, selected)
    };

    Ok(RunsPageView {
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        runs: if using_mock {
            build_run_list_items(&mock_runs(), Some(selected_run_id.clone()), "Mock preview")
        } else {
            build_run_list_items(&runs, Some(selected_run_id.clone()), "Live runtime")
        },
        selected: if using_mock {
            build_mock_run_detail(&selected_run_id)
        } else {
            build_live_run_detail(db, &selected_run_id)?
        },
    })
}

pub(super) fn mock_runs() -> Vec<OrchestrationRun> {
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

pub(super) fn build_run_list_items(
    runs: &[OrchestrationRun],
    selected_run_id: Option<String>,
    source_label: &str,
) -> Vec<RunListItem> {
    runs.iter()
        .map(|run| RunListItem {
            title: run.team_name.clone(),
            subtitle: format!("{} workflow · {}", run.workflow, source_label),
            updated_at: run.updated_at.clone(),
            progress_label: progress_label(run),
            badge: run.status.as_str().to_uppercase(),
            badge_tone: run_tone(&run.status),
            page_url: format!("/runs?run={}", encode(&run.team_run_id)),
            queue_page_url: format!("/queue?run={}", encode(&run.team_run_id)),
            active: selected_run_id
                .as_ref()
                .map(|selected| selected == &run.team_run_id)
                .unwrap_or(false),
        })
        .collect()
}

fn build_live_run_detail(db: Arc<Database>, run_id: &str) -> Result<RunDetailView> {
    let run_store = OrchestrationStore::new(db.clone());
    let work_store = WorkItemStore::new(db.clone());
    let queue = MessageQueue::new(db);

    let run = run_store
        .get_run(run_id)?
        .with_context(|| format!("run `{run_id}` not found"))?;
    let work_items = work_store.list_for_run(run_id, None)?;
    let broadcasts = queue.read_broadcasts(run_id, None)?;

    Ok(build_run_detail(
        &run,
        &work_items,
        &broadcasts,
        "Live runtime",
    ))
}

fn build_run_detail(
    run: &OrchestrationRun,
    work_items: &[WorkItem],
    broadcasts: &[QueueMessage],
    source_label: &str,
) -> RunDetailView {
    RunDetailView {
        title: format!("Run {}", run.team_run_id),
        subtitle: format!("{} / {}", run.team_name, run.workflow),
        source_label: source_label.into(),
        meta: vec![
            MetaRow {
                label: "Status".into(),
                value: run.status.as_str().into(),
            },
            MetaRow {
                label: "Progress".into(),
                value: progress_label(run),
            },
            MetaRow {
                label: "Session".into(),
                value: run.session_key.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: run.updated_at.clone(),
            },
        ],
        work_items: work_items
            .iter()
            .map(|item| WorkItemView {
                title: item.title.clone(),
                detail: item
                    .assigned_to
                    .clone()
                    .map(|assignee| format!("{assignee} · {}", item.updated_at))
                    .unwrap_or_else(|| item.updated_at.clone()),
                status_label: item.status.as_str().replace('_', " "),
                status_tone: work_tone(&item.status),
                step_label: item
                    .workflow_step
                    .map(|step| format!("Step {step}"))
                    .unwrap_or_else(|| "Root item".into()),
                indent_class: if item.parent_id.is_some() {
                    "is-child"
                } else {
                    "is-root"
                },
            })
            .collect(),
        broadcasts: broadcasts
            .iter()
            .map(|message| BroadcastView {
                sender: message.sender.clone(),
                created_at: message.created_at.clone(),
                content: message.content.clone(),
            })
            .collect(),
        input: run.input.clone(),
        result: run
            .result
            .clone()
            .unwrap_or_else(|| "No final result has been recorded yet.".into()),
        empty_hint: "No work items or broadcasts have been captured for this run yet.".into(),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use opengoose_persistence::{MessageStatus, MessageType, RunStatus, WorkStatus};

    use super::*;

    fn sample_run(id: &str, status: RunStatus) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: id.into(),
            session_key: "discord:ns:test:chan".into(),
            team_name: format!("team-{id}"),
            workflow: "chain".into(),
            input: "some input".into(),
            status,
            current_step: 1,
            total_steps: 3,
            result: None,
            created_at: "2026-03-10 10:00".into(),
            updated_at: "2026-03-10 10:05".into(),
        }
    }

    // --- mock_runs ---

    #[test]
    fn mock_runs_returns_three_entries() {
        let runs = mock_runs();
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn mock_runs_have_distinct_ids() {
        let runs = mock_runs();
        let ids: Vec<_> = runs.iter().map(|r| &r.team_run_id).collect();
        assert_eq!(ids[0], "run-preview-01");
        assert_eq!(ids[1], "run-preview-02");
        assert_eq!(ids[2], "run-preview-03");
    }

    // --- build_run_list_items ---

    #[test]
    fn build_run_list_items_active_flag_set_correctly() {
        let runs = vec![
            sample_run("r1", RunStatus::Running),
            sample_run("r2", RunStatus::Completed),
        ];
        let items = build_run_list_items(&runs, Some("r2".into()), "Live");
        assert!(!items[0].active);
        assert!(items[1].active);
    }

    #[test]
    fn build_run_list_items_no_selection_all_inactive() {
        let runs = vec![sample_run("r1", RunStatus::Running)];
        let items = build_run_list_items(&runs, None, "Mock");
        assert!(!items[0].active);
    }

    #[test]
    fn build_run_list_items_subtitle_includes_workflow_and_label() {
        let runs = vec![sample_run("r1", RunStatus::Running)];
        let items = build_run_list_items(&runs, None, "Mock preview");
        assert!(items[0].subtitle.contains("chain"));
        assert!(items[0].subtitle.contains("Mock preview"));
    }

    #[test]
    fn build_run_list_items_badge_is_uppercase_status() {
        let runs = vec![
            sample_run("r1", RunStatus::Running),
            sample_run("r2", RunStatus::Completed),
            sample_run("r3", RunStatus::Failed),
            sample_run("r4", RunStatus::Suspended),
        ];
        let items = build_run_list_items(&runs, None, "Live");
        assert_eq!(items[0].badge, "RUNNING");
        assert_eq!(items[1].badge, "COMPLETED");
        assert_eq!(items[2].badge, "FAILED");
        assert_eq!(items[3].badge, "SUSPENDED");
    }

    #[test]
    fn build_run_list_items_page_url_encodes_run_id() {
        let mut run = sample_run("run with spaces", RunStatus::Running);
        run.team_run_id = "run with spaces".into();
        let items = build_run_list_items(&[run], None, "Mock");
        assert!(
            items[0].page_url.contains("run+with+spaces")
                || items[0].page_url.contains("run%20with%20spaces")
        );
    }

    #[test]
    fn build_run_list_items_queue_page_url_uses_queue_path() {
        let runs = vec![sample_run("r1", RunStatus::Running)];
        let items = build_run_list_items(&runs, None, "Mock");
        assert!(items[0].queue_page_url.starts_with("/queue?run="));
    }

    // --- build_run_detail (via build_mock_run_detail) ---

    #[test]
    fn build_run_detail_title_contains_run_id() {
        let runs = mock_runs();
        let run = &runs[0];
        let work_items = vec![];
        let broadcasts = vec![];
        let detail = build_run_detail(run, &work_items, &broadcasts, "Mock");
        assert!(detail.title.contains(&run.team_run_id));
    }

    #[test]
    fn build_run_detail_subtitle_contains_team_and_workflow() {
        let runs = mock_runs();
        let run = &runs[0];
        let detail = build_run_detail(run, &[], &[], "Mock");
        assert!(detail.subtitle.contains(&run.team_name));
        assert!(detail.subtitle.contains(&run.workflow));
    }

    #[test]
    fn build_run_detail_result_fallback_when_none() {
        let runs = mock_runs();
        let run = &runs[0]; // status is Running, result is None
        let detail = build_run_detail(run, &[], &[], "Mock");
        assert!(detail.result.contains("No final result"));
    }

    #[test]
    fn build_run_detail_result_shown_when_some() {
        let mut run = sample_run("r1", RunStatus::Completed);
        run.result = Some("All done.".into());
        let detail = build_run_detail(&run, &[], &[], "Live");
        assert_eq!(detail.result, "All done.");
    }

    #[test]
    fn build_run_detail_work_item_root_vs_child_indent() {
        use opengoose_persistence::WorkItem;
        let runs = mock_runs();
        let run = &runs[0];
        let work_items = vec![
            WorkItem {
                id: 1,
                session_key: run.session_key.clone(),
                team_run_id: run.team_run_id.clone(),
                parent_id: None,
                title: "Root task".into(),
                description: None,
                status: WorkStatus::Pending,
                assigned_to: None,
                workflow_step: Some(0),
                input: None,
                output: None,
                error: None,
                hash_id: None,
                is_ephemeral: false,
                priority: 3,
                created_at: run.created_at.clone(),
                updated_at: run.updated_at.clone(),
            },
            WorkItem {
                id: 2,
                session_key: run.session_key.clone(),
                team_run_id: run.team_run_id.clone(),
                parent_id: Some(1),
                title: "Child task".into(),
                description: None,
                status: WorkStatus::InProgress,
                assigned_to: Some("agent".into()),
                workflow_step: Some(1),
                input: None,
                output: None,
                error: None,
                hash_id: None,
                is_ephemeral: false,
                priority: 3,
                created_at: run.created_at.clone(),
                updated_at: run.updated_at.clone(),
            },
        ];
        let detail = build_run_detail(run, &work_items, &[], "Mock");
        assert_eq!(detail.work_items[0].indent_class, "is-root");
        assert_eq!(detail.work_items[1].indent_class, "is-child");
    }

    #[test]
    fn build_run_detail_broadcast_mapped_correctly() {
        use opengoose_persistence::QueueMessage;
        let runs = mock_runs();
        let run = &runs[0];
        let broadcast = QueueMessage {
            id: 1,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            sender: "planner".into(),
            recipient: "broadcast".into(),
            content: "go ahead".into(),
            msg_type: MessageType::Broadcast,
            status: MessageStatus::Completed,
            retry_count: 0,
            max_retries: 3,
            created_at: run.created_at.clone(),
            processed_at: None,
            error: None,
        };
        let detail = build_run_detail(run, &[], &[broadcast], "Mock");
        assert_eq!(detail.broadcasts.len(), 1);
        assert_eq!(detail.broadcasts[0].sender, "planner");
        assert_eq!(detail.broadcasts[0].content, "go ahead");
    }

    #[test]
    fn build_run_detail_meta_rows_include_status_and_session() {
        let runs = mock_runs();
        let run = &runs[0];
        let detail = build_run_detail(run, &[], &[], "Mock");
        let labels: Vec<_> = detail.meta.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Status"));
        assert!(labels.contains(&"Session"));
        assert!(labels.contains(&"Progress"));
        assert!(labels.contains(&"Updated"));
    }
}

fn build_mock_run_detail(run_id: &str) -> RunDetailView {
    let runs = mock_runs();
    let run = runs
        .iter()
        .find(|run| run.team_run_id == run_id)
        .unwrap_or(&runs[0]);
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
            hash_id: None,
            is_ephemeral: false,
            priority: 3,
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
            hash_id: None,
            is_ephemeral: false,
            priority: 3,
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
    build_run_detail(run, &work_items, &broadcasts, "Mock preview")
}
