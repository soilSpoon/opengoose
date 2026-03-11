use crate::data::views::QueueDetailView;

use super::grouping::{build_queue_message_groups, build_queue_status_cards};
use super::loader::QueueDetailRecord;

pub(super) fn build_queue_detail(
    detail: &QueueDetailRecord,
    source_label: &str,
) -> QueueDetailView {
    let message_groups = build_queue_message_groups(&detail.messages, &detail.dead_letters);

    QueueDetailView {
        title: format!("Queue {}", detail.run.team_run_id),
        subtitle: format!("{} / {}", detail.run.team_name, detail.run.workflow),
        source_label: source_label.into(),
        status_cards: build_queue_status_cards(&detail.stats),
        messages: message_groups.messages,
        dead_letters: message_groups.dead_letters,
        empty_hint: "No queue traffic has been recorded for this run yet.".into(),
    }
}
