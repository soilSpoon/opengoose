use opengoose_persistence::{
    MessageStatus, MessageType, OrchestrationRun, QueueMessage, RunStatus, WorkItem, WorkStatus,
};

use super::loader::{mock_runs, RunDetailRecord};
use super::selection::{choose_selected_run_id, find_selected_run};
use super::view_model::{build_run_detail, build_run_list_items};

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

#[test]
fn mock_runs_returns_three_entries() {
    let runs = mock_runs();
    assert_eq!(runs.len(), 3);
}

#[test]
fn mock_runs_have_distinct_ids() {
    let runs = mock_runs();
    let ids: Vec<_> = runs.iter().map(|run| run.team_run_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["run-preview-01", "run-preview-02", "run-preview-03"]
    );
}

#[test]
fn choose_selected_run_id_returns_match() {
    let runs = vec![
        sample_run("r1", RunStatus::Running),
        sample_run("r2", RunStatus::Completed),
    ];

    assert_eq!(choose_selected_run_id(&runs, Some("r2".into())), "r2");
}

#[test]
fn choose_selected_run_id_falls_back_to_first() {
    let runs = vec![sample_run("r1", RunStatus::Running)];

    assert_eq!(
        choose_selected_run_id(&runs, Some("missing".into())),
        "r1".to_string()
    );
}

#[test]
fn find_selected_run_requires_existing_id() {
    let runs = vec![sample_run("r1", RunStatus::Running)];
    let error = find_selected_run(&runs, "missing").unwrap_err();

    assert!(error.to_string().contains("selected run missing"));
}

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
    let run = sample_run("run with spaces", RunStatus::Running);
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

#[test]
fn build_run_detail_title_contains_run_id() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run), "Mock");

    assert!(detail.title.contains("run-preview-01"));
}

#[test]
fn build_run_detail_subtitle_contains_team_and_workflow() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run.clone()), "Mock");

    assert!(detail.subtitle.contains(&run.team_name));
    assert!(detail.subtitle.contains(&run.workflow));
}

#[test]
fn build_run_detail_result_fallback_when_none() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run), "Mock");

    assert!(detail.result.contains("No final result"));
}

#[test]
fn build_run_detail_result_shown_when_some() {
    let mut run = sample_run("r1", RunStatus::Completed);
    run.result = Some("All done.".into());
    let detail = build_run_detail(&sample_run_detail(run), "Live");

    assert_eq!(detail.result, "All done.");
}

#[test]
fn build_run_detail_work_item_root_vs_child_indent() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run), "Mock");

    assert_eq!(detail.work_items[0].indent_class, "is-root");
    assert_eq!(detail.work_items[1].indent_class, "is-child");
}

#[test]
fn build_run_detail_broadcast_mapped_correctly() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run), "Mock");

    assert_eq!(detail.broadcasts.len(), 1);
    assert_eq!(detail.broadcasts[0].sender, "planner");
    assert_eq!(detail.broadcasts[0].content, "go ahead");
}

#[test]
fn build_run_detail_meta_rows_include_status_and_session() {
    let run = mock_runs().remove(0);
    let detail = build_run_detail(&sample_run_detail(run), "Mock");
    let labels: Vec<_> = detail.meta.iter().map(|row| row.label.as_str()).collect();

    assert!(labels.contains(&"Status"));
    assert!(labels.contains(&"Session"));
    assert!(labels.contains(&"Progress"));
    assert!(labels.contains(&"Updated"));
}

fn sample_run_detail(run: OrchestrationRun) -> RunDetailRecord {
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
            priority: 0,
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
            priority: 0,
            created_at: run.created_at.clone(),
            updated_at: run.updated_at.clone(),
        },
    ];
    let broadcasts = vec![QueueMessage {
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
    }];

    RunDetailRecord {
        run,
        work_items,
        broadcasts,
    }
}
