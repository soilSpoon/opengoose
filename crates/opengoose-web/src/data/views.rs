/// A single metric card rendered on the dashboard (label, value, footnote, tone).
#[derive(Clone)]
pub struct MetricCard {
    pub label: String,
    pub value: String,
    pub note: String,
    pub tone: &'static str,
}

/// An alert banner displayed on the dashboard.
#[derive(Clone)]
pub struct AlertCard {
    pub eyebrow: String,
    pub title: String,
    pub description: String,
    pub tone: &'static str,
}

/// One segment of a stacked status bar (e.g. "Running 3" at 40% width).
#[allow(dead_code)]
#[derive(Clone)]
pub struct StatusSegment {
    pub label: String,
    pub value: String,
    pub tone: &'static str,
    pub width: u8,
}

/// A single bar in the duration trend chart.
#[allow(dead_code)]
#[derive(Clone)]
pub struct TrendBar {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub tone: &'static str,
    pub height: u8,
}

/// One row in the activity feed timeline.
#[allow(dead_code)]
#[derive(Clone)]
pub struct ActivityItem {
    pub actor: String,
    pub meta: String,
    pub detail: String,
    pub timestamp: String,
    pub tone: &'static str,
}

/// A label/value metadata row shown in detail panels.
#[derive(Clone)]
pub struct MetaRow {
    pub label: String,
    pub value: String,
}

/// Summary row for the session list sidebar.
#[derive(Clone)]
pub struct SessionListItem {
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub updated_at: String,
    pub badge: String,
    pub badge_tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// A single chat message bubble in the session detail view.
#[derive(Clone)]
pub struct MessageBubble {
    pub role_label: String,
    pub author_label: String,
    pub timestamp: String,
    pub content: String,
    pub tone: &'static str,
    pub alignment: &'static str,
}

/// Full detail panel for a selected session, including messages and metadata.
#[derive(Clone)]
pub struct SessionDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub meta: Vec<MetaRow>,
    pub messages: Vec<MessageBubble>,
    pub empty_hint: String,
}

/// View-model for the sessions page (list + selected detail).
#[derive(Clone)]
pub struct SessionsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub sessions: Vec<SessionListItem>,
    pub selected: SessionDetailView,
}

/// Summary row for the orchestration run list sidebar.
#[derive(Clone)]
pub struct RunListItem {
    pub title: String,
    pub subtitle: String,
    pub updated_at: String,
    pub progress_label: String,
    pub badge: String,
    pub badge_tone: &'static str,
    pub page_url: String,
    pub queue_page_url: String,
    pub active: bool,
}

/// A single work item row in the run detail panel.
#[derive(Clone)]
pub struct WorkItemView {
    pub title: String,
    pub detail: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub step_label: String,
    pub indent_class: &'static str,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// A broadcast message shown in the run detail panel.
#[derive(Clone)]
pub struct BroadcastView {
    pub sender: String,
    pub created_at: String,
    pub content: String,
}

/// Full detail panel for a selected orchestration run.
#[derive(Clone)]
pub struct RunDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub detail_page_url: String,
    pub queue_page_url: String,
    pub meta: Vec<MetaRow>,
    pub work_items: Vec<WorkItemView>,
    pub broadcasts: Vec<BroadcastView>,
    pub input: String,
    pub result: String,
    pub empty_hint: String,
}

/// View-model for the runs page (list + selected detail).
#[derive(Clone)]
pub struct RunsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub runs: Vec<RunListItem>,
    pub selected: RunDetailView,
}

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

/// A configuration setting row in the agent detail panel.
#[derive(Clone)]
pub struct SettingRow {
    pub label: String,
    pub value: String,
}

/// An agent extension (skill entry) row in the agent detail panel.
#[derive(Clone)]
pub struct ExtensionRow {
    pub name: String,
    pub kind: String,
    pub summary: String,
}

/// Summary row for the agent list sidebar.
#[derive(Clone)]
pub struct AgentListItem {
    pub title: String,
    pub subtitle: String,
    pub capability: String,
    pub source_label: String,
    pub page_url: String,
    pub active: bool,
}

/// A recent orchestration run related to an agent profile.
#[derive(Clone)]
pub struct AgentRecentRunView {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
}

/// An active session routed through a workflow that uses an agent profile.
#[derive(Clone)]
pub struct AgentSessionView {
    pub title: String,
    pub detail: String,
    pub updated_at: String,
    pub badge: String,
    pub badge_tone: &'static str,
    pub page_url: String,
}

/// Full detail panel for a selected agent profile.
#[derive(Clone)]
pub struct AgentDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub detail_page_url: String,
    pub instructions_preview: String,
    pub settings: Vec<SettingRow>,
    pub activities: Vec<String>,
    pub skills: Vec<String>,
    pub extensions: Vec<ExtensionRow>,
    pub recent_runs: Vec<AgentRecentRunView>,
    pub connected_sessions: Vec<AgentSessionView>,
    pub runtime_empty_hint: String,
    pub yaml: String,
}

/// View-model for the agents page (list + selected detail).
#[derive(Clone)]
pub struct AgentsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub agents: Vec<AgentListItem>,
    pub selected: AgentDetailView,
}

/// Summary row for the team list sidebar.
#[derive(Clone)]
pub struct TeamListItem {
    pub title: String,
    pub subtitle: String,
    pub members: String,
    pub source_label: String,
    pub page_url: String,
    pub active: bool,
}

/// A toast-style notice shown after an action (e.g. team save).
#[derive(Clone)]
pub struct Notice {
    pub text: String,
    pub tone: &'static str,
}

/// Detail/editor panel for a selected team definition.
#[derive(Clone)]
pub struct TeamEditorView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub workflow_label: String,
    pub members_text: String,
    pub original_name: String,
    pub yaml: String,
    pub notice: Option<Notice>,
}

/// View-model for the teams page (list + selected editor).
#[derive(Clone)]
pub struct TeamsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub teams: Vec<TeamListItem>,
    pub selected: TeamEditorView,
}

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

/// Option row for a `<select>` field.
#[derive(Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
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

/// A gateway connection status card for the dashboard widget.
#[derive(Clone)]
pub struct GatewayCard {
    pub platform: String,
    pub state_label: String,
    pub state_tone: &'static str,
    pub uptime_label: String,
    pub detail: String,
}

/// Aggregated view-model for the main dashboard page.
#[allow(dead_code)]
#[derive(Clone)]
pub struct DashboardView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub stream_summary: String,
    pub snapshot_label: String,
    pub metrics: Vec<MetricCard>,
    pub queue_cards: Vec<MetricCard>,
    pub run_segments: Vec<StatusSegment>,
    pub queue_segments: Vec<StatusSegment>,
    pub duration_bars: Vec<TrendBar>,
    pub activities: Vec<ActivityItem>,
    pub alerts: Vec<AlertCard>,
    pub sessions: Vec<SessionListItem>,
    pub runs: Vec<RunListItem>,
    pub gateways: Vec<GatewayCard>,
}
