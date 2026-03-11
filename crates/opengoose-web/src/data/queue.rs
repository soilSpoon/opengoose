use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, MessageQueue, OrchestrationRun, OrchestrationStore, QueueMessage, QueueStats,
};

use crate::data::runs::{build_run_list_items, mock_runs};
use crate::data::utils::{choose_selected_run, queue_tone};
use crate::data::views::{MetricCard, QueueDetailView, QueueMessageView, QueuePageView};

/// Load the queue page view-model, optionally selecting a run by ID.
pub fn load_queue_page(db: Arc<Database>, selected: Option<String>) -> Result<QueuePageView> {
    let run_store = OrchestrationStore::new(db.clone());
    let runs = run_store.list_runs(None, 20)?;
    let using_mock = runs.is_empty();
    let selected_run_id = if using_mock {
        choose_selected_run(&mock_runs(), selected)
    } else {
        choose_selected_run(&runs, selected)
    };

    Ok(QueuePageView {
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
            build_mock_queue_detail(&selected_run_id)
        } else {
            build_live_queue_detail(db, &selected_run_id)?
        },
    })
}

fn build_live_queue_detail(db: Arc<Database>, run_id: &str) -> Result<QueueDetailView> {
    let run_store = OrchestrationStore::new(db.clone());
    let queue = MessageQueue::new(db);
    let run = run_store
        .get_run(run_id)?
        .with_context(|| format!("run `{run_id}` not found"))?;
    let messages = queue.list_for_run(run_id)?;
    let dead_letters = queue.get_dead_letters(run_id)?;
    let stats = queue.stats()?;

    Ok(build_queue_detail(
        &run,
        &messages,
        &dead_letters,
        &stats,
        "Live runtime",
    ))
}

fn build_mock_queue_detail(run_id: &str) -> QueueDetailView {
    let runs = mock_runs();
    let run = runs
        .iter()
        .find(|run| run.team_run_id == run_id)
        .unwrap_or(&runs[0]);
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
    build_queue_detail(run, &messages, &[], &stats, "Mock preview")
}

fn build_queue_detail(
    run: &OrchestrationRun,
    messages: &[QueueMessage],
    dead_letters: &[QueueMessage],
    stats: &QueueStats,
    source_label: &str,
) -> QueueDetailView {
    QueueDetailView {
        title: format!("Queue {}", run.team_run_id),
        subtitle: format!("{} / {}", run.team_name, run.workflow),
        source_label: source_label.into(),
        status_cards: vec![
            MetricCard {
                label: "Pending".into(),
                value: stats.pending.to_string(),
                note: "Waiting for recipients".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Processing".into(),
                value: stats.processing.to_string(),
                note: "Locked for execution".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Completed".into(),
                value: stats.completed.to_string(),
                note: "Already resolved".into(),
                tone: "sage",
            },
            MetricCard {
                label: "Dead".into(),
                value: stats.dead.to_string(),
                note: "Needs manual intervention".into(),
                tone: "rose",
            },
        ],
        messages: messages.iter().map(build_queue_row).collect(),
        dead_letters: dead_letters.iter().map(build_queue_row).collect(),
        empty_hint: "No queue traffic has been recorded for this run yet.".into(),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use opengoose_persistence::{MessageStatus, MessageType, QueueMessage, QueueStats};

    use super::*;

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
            status: opengoose_persistence::RunStatus::Running,
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

    // --- build_queue_row ---

    #[test]
    fn build_queue_row_retry_text_format() {
        let msg = sample_message(1, MessageType::Task, MessageStatus::Pending);
        let row = build_queue_row(&msg);
        assert_eq!(row.retry_text, "1/3");
    }

    #[test]
    fn build_queue_row_kind_underscores_replaced() {
        let msg = sample_message(1, MessageType::Task, MessageStatus::Pending);
        let row = build_queue_row(&msg);
        assert!(!row.kind.contains('_'));
    }

    #[test]
    fn build_queue_row_status_label_underscores_replaced() {
        let msg = sample_message(1, MessageType::Task, MessageStatus::Processing);
        let row = build_queue_row(&msg);
        assert!(!row.status_label.contains('_'));
    }

    #[test]
    fn build_queue_row_error_empty_when_none() {
        let msg = sample_message(1, MessageType::Task, MessageStatus::Pending);
        let row = build_queue_row(&msg);
        assert_eq!(row.error, "");
    }

    #[test]
    fn build_queue_row_error_shown_when_some() {
        let mut msg = sample_message(1, MessageType::Task, MessageStatus::Failed);
        msg.error = Some("timed out".into());
        let row = build_queue_row(&msg);
        assert_eq!(row.error, "timed out");
    }

    #[test]
    fn build_queue_row_status_tone_mapping() {
        let pending = build_queue_row(&sample_message(
            1,
            MessageType::Task,
            MessageStatus::Pending,
        ));
        let completed = build_queue_row(&sample_message(
            2,
            MessageType::Task,
            MessageStatus::Completed,
        ));
        let failed = build_queue_row(&sample_message(3, MessageType::Task, MessageStatus::Failed));
        assert_eq!(pending.status_tone, "amber");
        assert_eq!(completed.status_tone, "sage");
        assert_eq!(failed.status_tone, "rose");
    }

    // --- build_queue_detail ---

    #[test]
    fn build_queue_detail_title_contains_run_id() {
        let run = sample_run("my-run");
        let detail = build_queue_detail(&run, &[], &[], &sample_stats(0, 0), "Mock");
        assert!(detail.title.contains("my-run"));
    }

    #[test]
    fn build_queue_detail_subtitle_contains_team_and_workflow() {
        let run = sample_run("r1");
        let detail = build_queue_detail(&run, &[], &[], &sample_stats(0, 0), "Mock");
        assert!(detail.subtitle.contains("test-team"));
        assert!(detail.subtitle.contains("chain"));
    }

    #[test]
    fn build_queue_detail_status_cards_count() {
        let run = sample_run("r1");
        let detail = build_queue_detail(&run, &[], &[], &sample_stats(1, 5), "Mock");
        assert_eq!(detail.status_cards.len(), 4);
    }

    #[test]
    fn build_queue_detail_status_cards_pending_value() {
        let run = sample_run("r1");
        let stats = QueueStats {
            pending: 7,
            processing: 0,
            completed: 0,
            failed: 0,
            dead: 0,
        };
        let detail = build_queue_detail(&run, &[], &[], &stats, "Mock");
        let pending_card = detail
            .status_cards
            .iter()
            .find(|c| c.label == "Pending")
            .unwrap();
        assert_eq!(pending_card.value, "7");
    }

    #[test]
    fn build_queue_detail_messages_mapped() {
        let run = sample_run("r1");
        let msgs = vec![
            sample_message(1, MessageType::Task, MessageStatus::Pending),
            sample_message(2, MessageType::Broadcast, MessageStatus::Completed),
        ];
        let detail = build_queue_detail(&run, &msgs, &[], &sample_stats(0, 0), "Mock");
        assert_eq!(detail.messages.len(), 2);
    }

    #[test]
    fn build_queue_detail_dead_letters_mapped_separately() {
        let run = sample_run("r1");
        let dead = vec![sample_message(10, MessageType::Task, MessageStatus::Dead)];
        let detail = build_queue_detail(&run, &[], &dead, &sample_stats(0, 0), "Mock");
        assert_eq!(detail.dead_letters.len(), 1);
        assert_eq!(detail.messages.len(), 0);
    }

    // --- build_mock_queue_detail ---

    #[test]
    fn build_mock_queue_detail_has_source_label_mock_preview() {
        let detail = build_mock_queue_detail("run-preview-01");
        assert_eq!(detail.source_label, "Mock preview");
    }

    #[test]
    fn build_mock_queue_detail_has_two_messages() {
        let detail = build_mock_queue_detail("run-preview-01");
        assert_eq!(detail.messages.len(), 2);
    }

    #[test]
    fn build_mock_queue_detail_unknown_run_id_falls_back_to_first() {
        let detail = build_mock_queue_detail("nonexistent-run");
        // Should fall back to the first mock run (run-preview-01)
        assert!(detail.title.contains("run-preview-01"));
    }
}

fn build_queue_row(message: &QueueMessage) -> QueueMessageView {
    QueueMessageView {
        sender: message.sender.clone(),
        recipient: message.recipient.clone(),
        kind: message.msg_type.as_str().replace('_', " "),
        status_label: message.status.as_str().replace('_', " "),
        status_tone: queue_tone(&message.status),
        created_at: message.created_at.clone(),
        retry_text: format!("{}/{}", message.retry_count, message.max_retries),
        content: message.content.clone(),
        error: message.error.clone().unwrap_or_default(),
    }
}
