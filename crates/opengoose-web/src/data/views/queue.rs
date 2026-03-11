use super::runs::RunListItem;
use super::shared::MetricCard;

/// A single inter-agent message row in the queue detail table.
#[derive(Clone)]
pub struct QueueMessageView {
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub created_at: String,
    pub retry_text: String,
    pub content: String,
    pub error: String,
}

/// Full detail panel for a selected message queue run.
#[derive(Clone)]
pub struct QueueDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub status_cards: Vec<MetricCard>,
    pub messages: Vec<QueueMessageView>,
    pub dead_letters: Vec<QueueMessageView>,
    pub empty_hint: String,
}

/// View-model for the queue page (run list + selected detail).
#[derive(Clone)]
pub struct QueuePageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub runs: Vec<RunListItem>,
    pub selected: QueueDetailView,
}
