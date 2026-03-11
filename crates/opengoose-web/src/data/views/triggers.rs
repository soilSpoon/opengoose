use super::shared::MetaRow;

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
    pub delete_api_url: String,
    pub toggle_enabled_api_url: String,
    pub test_api_url: String,
    pub update_api_url: String,
    pub is_placeholder: bool,
}

/// View-model for the triggers page (list + selected detail).
#[derive(Clone)]
pub struct TriggersPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub triggers: Vec<TriggerListItem>,
    pub selected: TriggerDetailView,
    pub create_api_url: String,
}
