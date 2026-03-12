use std::sync::Arc;

use opengoose_persistence::{
    Database, MessageQueue, MessageStatus, MessageType, OrchestrationRun, OrchestrationStore,
    QueueMessage, QueueStats, RunStatus,
};

use super::detail::build_queue_detail;
use super::grouping::{build_queue_message_groups, build_queue_status_cards};
use super::load_queue_page;
use super::loader::{load_queue_detail, load_queue_runs, mock_queue_detail};

fn sample_message(id: i32, msg_type: MessageType, status: MessageStatus) -> QueueMessage {
    QueueMessage {
        id,
        session_key: "discord:ns:test:chan".into(),
        team_run_id: "run-1".into(),
        sender: "planner".into(),
        recipient: "developer".into(),
        content: "do the work".into(),
        msg_type,
        status,
        retry_count: 1,
        max_retries: 3,
        created_at: "2026-03-10 10:00".into(),
        processed_at: None,
        error: None,
    }
}

fn sample_run(id: &str) -> OrchestrationRun {
    OrchestrationRun {
        team_run_id: id.into(),
        session_key: "discord:ns:test:chan".into(),
        team_name: "test-team".into(),
        workflow: "chain".into(),
        input: "some input".into(),
        status: RunStatus::Running,
        current_step: 1,
        total_steps: 2,
        result: None,
        created_at: "2026-03-10 10:00".into(),
        updated_at: "2026-03-10 10:05".into(),
    }
}

fn sample_stats(pending: i64, completed: i64) -> QueueStats {
    QueueStats {
        pending,
        processing: 0,
        completed,
        failed: 0,
        dead: 0,
    }
}

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

fn seed_live_run(db: &Arc<Database>, run_id: &str) -> String {
    let session_key = format!("discord:ns:test:{run_id}");
    OrchestrationStore::new(db.clone())
        .create_run(run_id, &session_key, "test-team", "chain", "some input", 2)
        .unwrap();
    session_key
}

#[test]
fn build_queue_message_groups_retry_text_format() {
    let groups = build_queue_message_groups(
        &[sample_message(1, MessageType::Task, MessageStatus::Pending)],
        &[],
    );

    assert_eq!(groups.messages[0].retry_text, "1/3");
}

#[test]
fn build_queue_message_groups_kind_underscores_replaced() {
    let groups = build_queue_message_groups(
        &[sample_message(1, MessageType::Task, MessageStatus::Pending)],
        &[],
    );

    assert!(!groups.messages[0].kind.contains('_'));
}

#[test]
fn build_queue_message_groups_status_label_underscores_replaced() {
    let groups = build_queue_message_groups(
        &[sample_message(
            1,
            MessageType::Task,
            MessageStatus::Processing,
        )],
        &[],
    );

    assert!(!groups.messages[0].status_label.contains('_'));
}

#[test]
fn build_queue_message_groups_error_empty_when_none() {
    let groups = build_queue_message_groups(
        &[sample_message(1, MessageType::Task, MessageStatus::Pending)],
        &[],
    );

    assert_eq!(groups.messages[0].error, "");
}

#[test]
fn build_queue_message_groups_error_shown_when_some() {
    let mut message = sample_message(1, MessageType::Task, MessageStatus::Failed);
    message.error = Some("timed out".into());
    let groups = build_queue_message_groups(&[message], &[]);

    assert_eq!(groups.messages[0].error, "timed out");
}

#[test]
fn build_queue_message_groups_status_tone_mapping() {
    let pending = build_queue_message_groups(
        &[sample_message(1, MessageType::Task, MessageStatus::Pending)],
        &[],
    );
    let completed = build_queue_message_groups(
        &[sample_message(
            2,
            MessageType::Task,
            MessageStatus::Completed,
        )],
        &[],
    );
    let failed = build_queue_message_groups(
        &[sample_message(3, MessageType::Task, MessageStatus::Failed)],
        &[],
    );

    assert_eq!(pending.messages[0].status_tone, "amber");
    assert_eq!(completed.messages[0].status_tone, "sage");
    assert_eq!(failed.messages[0].status_tone, "rose");
}

#[test]
fn build_queue_status_cards_count() {
    let cards = build_queue_status_cards(&sample_stats(1, 5));

    assert_eq!(cards.len(), 4);
}

#[test]
fn build_queue_status_cards_pending_value() {
    let cards = build_queue_status_cards(&QueueStats {
        pending: 7,
        processing: 0,
        completed: 0,
        failed: 0,
        dead: 0,
    });
    let pending_card = cards.iter().find(|card| card.label == "Pending").unwrap();

    assert_eq!(pending_card.value, "7");
}

#[test]
fn build_queue_detail_title_contains_run_id() {
    let run = sample_run("my-run");
    let detail = build_queue_detail(&mock_queue_detail(&run), "Mock preview");

    assert!(detail.title.contains("my-run"));
}

#[test]
fn build_queue_detail_subtitle_contains_team_and_workflow() {
    let run = sample_run("r1");
    let detail = build_queue_detail(&mock_queue_detail(&run), "Mock preview");

    assert!(detail.subtitle.contains("test-team"));
    assert!(detail.subtitle.contains("chain"));
}

#[test]
fn build_queue_detail_uses_source_label() {
    let run = sample_run("r1");
    let detail = build_queue_detail(&mock_queue_detail(&run), "Mock preview");

    assert_eq!(detail.source_label, "Mock preview");
}

#[test]
fn build_queue_detail_messages_mapped() {
    let run = sample_run("r1");
    let detail = build_queue_detail(&mock_queue_detail(&run), "Mock preview");

    assert_eq!(detail.messages.len(), 2);
}

#[test]
fn build_queue_detail_dead_letters_mapped_separately() {
    let run = sample_run("r1");
    let detail = build_queue_detail(
        &super::loader::QueueDetailRecord {
            run,
            messages: Vec::new(),
            dead_letters: vec![sample_message(10, MessageType::Task, MessageStatus::Dead)],
            stats: sample_stats(0, 0),
        },
        "Mock preview",
    );

    assert_eq!(detail.dead_letters.len(), 1);
    assert_eq!(detail.messages.len(), 0);
}

#[test]
fn load_queue_page_uses_mock_preview_when_no_runs() {
    let page = load_queue_page(test_db(), None).unwrap();

    assert_eq!(page.mode_label, "Mock preview");
    assert_eq!(page.mode_tone, "neutral");
    assert!(page.selected.title.contains("run-preview-01"));
}

#[test]
fn load_queue_page_resolves_requested_mock_run() {
    let page = load_queue_page(test_db(), Some("run-preview-03".into())).unwrap();

    assert!(page.selected.title.contains("run-preview-03"));
    assert_eq!(page.runs.iter().filter(|item| item.active).count(), 1);
    assert!(
        page.runs
            .iter()
            .any(|item| item.active && item.page_url.contains("run-preview-03"))
    );
}

#[test]
fn load_queue_page_live_runtime_loads_selected_run_messages() {
    let db = test_db();
    let session_key = seed_live_run(&db, "run-live-01");
    let other_session_key = seed_live_run(&db, "run-live-02");
    let queue = MessageQueue::new(db.clone());

    queue
        .enqueue(
            &session_key,
            "run-live-01",
            "planner",
            "developer",
            "first queue item",
            MessageType::Task,
        )
        .unwrap();
    queue
        .enqueue(
            &other_session_key,
            "run-live-02",
            "planner",
            "reviewer",
            "selected queue item",
            MessageType::Task,
        )
        .unwrap();

    let page = load_queue_page(db, Some("run-live-02".into())).unwrap();

    assert_eq!(page.mode_label, "Live runtime");
    assert_eq!(page.mode_tone, "success");
    assert!(page.selected.title.contains("run-live-02"));
    assert_eq!(page.selected.messages.len(), 1);
    assert_eq!(page.selected.messages[0].content, "selected queue item");
}

#[test]
fn load_queue_detail_returns_mock_preview_records() {
    let db = test_db();
    let loaded = load_queue_runs(db.clone(), 20).unwrap();
    let detail = load_queue_detail(db, &loaded, "run-preview-02").unwrap();

    assert_eq!(detail.run.team_run_id, "run-preview-02");
    assert_eq!(detail.messages.len(), 2);
}
