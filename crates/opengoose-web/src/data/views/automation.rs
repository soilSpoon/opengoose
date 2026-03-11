use super::{MetaRow, Notice, SelectOption};

/// Summary row for the schedule list sidebar.
#[derive(Clone)]
pub struct ScheduleListItem {
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub source_label: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// A recent run associated with a selected schedule.
#[derive(Clone)]
pub struct ScheduleHistoryItem {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
}

/// Detail/editor panel for a selected schedule definition.
#[derive(Clone)]
pub struct ScheduleEditorView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub original_name: String,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
    pub is_new: bool,
    pub name_locked: bool,
    pub meta: Vec<MetaRow>,
    pub team_options: Vec<SelectOption>,
    pub history: Vec<ScheduleHistoryItem>,
    pub history_hint: String,
    pub notice: Option<Notice>,
    pub save_label: String,
    pub toggle_label: String,
    pub delete_label: String,
}

/// View-model for the schedules page (list + selected editor).
#[derive(Clone)]
pub struct SchedulesPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub schedules: Vec<ScheduleListItem>,
    pub selected: ScheduleEditorView,
    pub new_schedule_url: String,
}

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

/// Summary row for the trigger list sidebar.
#[derive(Clone)]
pub struct TriggerListItem {
    pub title: String,
    pub subtitle: String,
    pub team_label: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub last_fired: String,
    pub page_url: String,
    pub active: bool,
}

/// Full detail/action panel for a selected trigger.
#[derive(Clone)]
pub struct TriggerDetailView {
    pub name: String,
    pub trigger_type: String,
    pub team_name: String,
    pub input: String,
    pub condition_json: String,
    pub enabled: bool,
    pub fire_count: i32,
    pub last_fired_at: String,
    pub created_at: String,
    pub meta: Vec<MetaRow>,
    pub status_label: String,
    pub status_tone: &'static str,
    pub notice: Option<Notice>,
    pub is_placeholder: bool,
}

/// View-model for the triggers page (list + selected detail).
#[derive(Clone)]
pub struct TriggersPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub triggers: Vec<TriggerListItem>,
    pub selected: TriggerDetailView,
}
