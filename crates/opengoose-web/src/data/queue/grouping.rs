use opengoose_persistence::{QueueMessage, QueueStats};

use crate::data::utils::queue_tone;
use crate::data::views::{MetricCard, QueueMessageView};

pub(super) struct QueueMessageGroups {
    pub(super) messages: Vec<QueueMessageView>,
    pub(super) dead_letters: Vec<QueueMessageView>,
}

pub(super) fn build_queue_status_cards(stats: &QueueStats) -> Vec<MetricCard> {
    vec![
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
    ]
}

pub(super) fn build_queue_message_groups(
    messages: &[QueueMessage],
    dead_letters: &[QueueMessage],
) -> QueueMessageGroups {
    QueueMessageGroups {
        messages: messages.iter().map(build_queue_row).collect(),
        dead_letters: dead_letters.iter().map(build_queue_row).collect(),
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
