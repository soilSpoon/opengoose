use super::shared::{MetaRow, Notice, SelectOption};

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
