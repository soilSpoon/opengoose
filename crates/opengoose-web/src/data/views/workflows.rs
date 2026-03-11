use super::shared::MetaRow;

/// Summary row for the workflow list sidebar.
#[derive(Clone)]
pub struct WorkflowListItem {
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub source_label: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// A single agent step in a workflow definition.
#[derive(Clone)]
pub struct WorkflowStepView {
    pub title: String,
    pub detail: String,
    pub badge: String,
    pub badge_tone: &'static str,
}

/// A schedule or trigger attached to a workflow.
#[derive(Clone)]
pub struct WorkflowAutomationView {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub note: String,
    pub status_label: String,
    pub status_tone: &'static str,
}

/// A recent orchestration run for a workflow.
#[derive(Clone)]
pub struct WorkflowRunView {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
}

/// Full detail panel for a selected workflow definition.
#[derive(Clone)]
pub struct WorkflowDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub meta: Vec<MetaRow>,
    pub steps: Vec<WorkflowStepView>,
    pub automations: Vec<WorkflowAutomationView>,
    pub recent_runs: Vec<WorkflowRunView>,
    pub yaml: String,
    pub trigger_api_url: String,
    pub trigger_input: String,
}

/// View-model for the workflows page (list + selected detail).
#[derive(Clone)]
pub struct WorkflowsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub workflows: Vec<WorkflowListItem>,
    pub selected: WorkflowDetailView,
}
