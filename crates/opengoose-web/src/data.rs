use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use chrono::{NaiveDateTime, Utc};
use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, HistoryMessage, MessageQueue,
    OrchestrationRun, OrchestrationStore, QueueMessage, QueueStats, RunStatus, Schedule,
    ScheduleStore, SessionStore, SessionSummary, Trigger, TriggerStore, WorkItem, WorkItemStore,
    WorkStatus,
};
use opengoose_profiles::{AgentProfile, ProfileStore, all_defaults as default_profiles};
use opengoose_teams::triggers::TriggerType;
use opengoose_teams::{TeamDefinition, TeamStore, all_defaults as default_teams};
use opengoose_types::SessionKey;
use urlencoding::encode;

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

/// Full detail panel for a selected agent profile.
#[derive(Clone)]
pub struct AgentDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub instructions_preview: String,
    pub settings: Vec<SettingRow>,
    pub activities: Vec<String>,
    pub skills: Vec<String>,
    pub extensions: Vec<ExtensionRow>,
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
    pub manage_triggers_url: String,
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

/// One workflow option in the trigger creation form.
#[derive(Clone)]
pub struct TriggerWorkflowOptionView {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub selected: bool,
}

/// One trigger type option in the trigger creation form.
#[derive(Clone)]
pub struct TriggerTypeOptionView {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

/// Draft values for the trigger creation form.
#[derive(Clone)]
pub struct TriggerDraftView {
    pub name: String,
    pub trigger_type: String,
    pub workflow_name: String,
    pub condition_json: String,
    pub condition_help: String,
    pub input: String,
}

/// One trigger row rendered in the management table.
#[derive(Clone)]
pub struct TriggerListItemView {
    pub name: String,
    pub trigger_type: String,
    pub trigger_type_label: String,
    pub workflow_title: String,
    pub workflow_page_url: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub fire_count: i32,
    pub last_fired_at: String,
    pub condition_preview: String,
    pub input_preview: String,
    pub toggle_label: String,
    pub toggle_enabled_value: bool,
    pub search_text: String,
}

/// View-model for the trigger management page.
#[derive(Clone)]
pub struct TriggersPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub notice: Option<Notice>,
    pub summary: Vec<MetricCard>,
    pub form: TriggerDraftView,
    pub workflows: Vec<TriggerWorkflowOptionView>,
    pub trigger_types: Vec<TriggerTypeOptionView>,
    pub triggers: Vec<TriggerListItemView>,
    pub empty_hint: String,
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
}

/// Load all data needed for the dashboard page from the database.
pub fn load_dashboard(db: Arc<Database>) -> Result<DashboardView> {
    let session_store = SessionStore::new(db.clone());
    let session_stats = session_store.stats()?;
    let session_rows = session_store.list_sessions(4)?;

    let run_store = OrchestrationStore::new(db.clone());
    let recent_runs = run_store.list_runs(None, 12)?;

    let queue = MessageQueue::new(db.clone());
    let queue_stats = queue.stats()?;

    let using_mock = session_rows.is_empty()
        && recent_runs.is_empty()
        && queue_total(&queue_stats) == 0
        && session_stats.session_count == 0;

    let source_label = if using_mock {
        "Mock preview"
    } else {
        "Live runtime"
    };

    let session_records = if using_mock {
        mock_sessions()
    } else {
        live_sessions(&session_store, &session_rows)?
    };
    let run_records = if using_mock { mock_runs() } else { recent_runs };

    let running_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Running)
        .count();
    let completed_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Completed)
        .count();
    let failed_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Failed)
        .count();
    let suspended_count = run_records
        .iter()
        .filter(|run| run.status == RunStatus::Suspended)
        .count();
    let terminal_total = completed_count + failed_count;
    let success_rate = ratio_percent(completed_count, terminal_total);
    let duration_stats = duration_stats(&run_records);
    let queue_backlog = queue_stats.pending + queue_stats.processing;

    let sessions = build_session_list_items(&session_records, None, source_label);
    let run_items = build_run_list_items(&run_records, None, source_label);
    let activities =
        build_dashboard_activities(db, &run_records, &session_records, &queue_stats, using_mock)?;

    let mut alerts = Vec::new();
    if using_mock {
        alerts.push(AlertCard {
            eyebrow: "Preview Mode".into(),
            title: "No runtime data yet".into(),
            description: "The dashboard is rendering seeded sessions, runs, and queue traffic so the UI can be reviewed before live traffic exists.".into(),
            tone: "neutral",
        });
    }
    if queue_stats.dead > 0 {
        alerts.push(AlertCard {
            eyebrow: "Queue Alert".into(),
            title: format!("{} dead-letter item(s)", queue_stats.dead),
            description: "Review the queue monitor to inspect retries and failed delegations."
                .into(),
            tone: "danger",
        });
    }
    if queue_backlog > 0 {
        alerts.push(AlertCard {
            eyebrow: "Queue Flow".into(),
            title: format!("{queue_backlog} item(s) still in flight"),
            description: "Pending and processing traffic is visible in the queue monitor and refreshes automatically through the SSE stream.".into(),
            tone: "amber",
        });
    }
    if terminal_total > 0 && success_rate < 80 {
        alerts.push(AlertCard {
            eyebrow: "Execution Risk".into(),
            title: format!("Success rate at {success_rate}%"),
            description: "Recent finished runs have started to trend down. Review the activity feed and queue errors before the backlog compounds.".into(),
            tone: "danger",
        });
    } else if !using_mock && running_count > 0 {
        alerts.push(AlertCard {
            eyebrow: "Runtime Active".into(),
            title: format!("{running_count} orchestration run(s) currently active"),
            description: "The dashboard is streaming run status, queue pressure, and agent chatter from the persisted runtime state.".into(),
            tone: "success",
        });
    }
    if alerts.is_empty() {
        alerts.push(AlertCard {
            eyebrow: "Steady State".into(),
            title: "No critical alerts in the latest snapshot".into(),
            description: "The dashboard remains live and ready for the next orchestration burst."
                .into(),
            tone: "neutral",
        });
    }

    Ok(DashboardView {
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        stream_summary: if using_mock {
            "The dashboard is rendering seeded signals so the monitoring layout can be reviewed before live traffic exists.".into()
        } else {
            "Server-sent events stream fresh snapshots from SQLite-backed sessions, runs, queue traffic, and agent chatter every few seconds.".into()
        },
        snapshot_label: format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC")),
        metrics: vec![
            MetricCard {
                label: "Tracked sessions".into(),
                value: session_stats.session_count.to_string(),
                note: format!("{} stored messages", session_stats.message_count),
                tone: "cyan",
            },
            MetricCard {
                label: "Active runs".into(),
                value: running_count.to_string(),
                note: format!("{completed_count} completed in the latest window"),
                tone: "amber",
            },
            MetricCard {
                label: "Success rate".into(),
                value: if terminal_total == 0 {
                    "No finished runs".into()
                } else {
                    format!("{success_rate}%")
                },
                note: format!("{completed_count} complete / {failed_count} failed"),
                tone: "sage",
            },
            MetricCard {
                label: "Avg cycle".into(),
                value: duration_stats
                    .average_label
                    .unwrap_or_else(|| "Awaiting data".into()),
                note: duration_stats.note,
                tone: "rose",
            },
        ],
        queue_cards: vec![
            MetricCard {
                label: "Pending".into(),
                value: queue_stats.pending.to_string(),
                note: "Waiting for pickup".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Processing".into(),
                value: queue_stats.processing.to_string(),
                note: "Currently being handled".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Completed".into(),
                value: queue_stats.completed.to_string(),
                note: "Resolved queue items".into(),
                tone: "sage",
            },
            MetricCard {
                label: "Dead letters".into(),
                value: queue_stats.dead.to_string(),
                note: "Needs operator attention".into(),
                tone: "rose",
            },
        ],
        run_segments: build_status_segments(vec![
            ("Running", running_count as i64, "cyan"),
            ("Completed", completed_count as i64, "sage"),
            ("Suspended", suspended_count as i64, "amber"),
            ("Failed", failed_count as i64, "rose"),
        ]),
        queue_segments: build_status_segments(vec![
            ("Pending", queue_stats.pending, "amber"),
            ("Processing", queue_stats.processing, "cyan"),
            ("Completed", queue_stats.completed, "sage"),
            ("Dead", queue_stats.dead, "rose"),
        ]),
        duration_bars: build_duration_bars(&run_records),
        activities,
        alerts,
        sessions,
        runs: run_items,
    })
}

/// Load the sessions page view-model, optionally selecting a session by key.
pub fn load_sessions_page(db: Arc<Database>, selected: Option<String>) -> Result<SessionsPageView> {
    let store = SessionStore::new(db);
    let session_rows = store.list_sessions(24)?;
    let using_mock = session_rows.is_empty();

    let sessions = if using_mock {
        mock_sessions()
    } else {
        live_sessions(&store, &session_rows)?
    };
    let selected_key = choose_selected_session(&sessions, selected);

    Ok(SessionsPageView {
        mode_label: if using_mock {
            "Mock preview".into()
        } else {
            "Live runtime".into()
        },
        mode_tone: if using_mock { "neutral" } else { "success" },
        sessions: build_session_list_items(
            &sessions,
            Some(selected_key.clone()),
            if using_mock {
                "Mock preview"
            } else {
                "Live runtime"
            },
        ),
        selected: build_session_detail(
            sessions
                .iter()
                .find(|session| session.summary.session_key == selected_key)
                .context("selected session missing")?,
            if using_mock {
                "Mock preview"
            } else {
                "Live runtime"
            },
        ),
    })
}

/// Load the detail panel for a single session.
pub fn load_session_detail(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<SessionDetailView> {
    Ok(load_sessions_page(db, selected)?.selected)
}

/// Load the runs page view-model, optionally selecting a run by ID.
pub fn load_runs_page(db: Arc<Database>, selected: Option<String>) -> Result<RunsPageView> {
    let run_store = OrchestrationStore::new(db.clone());
    let runs = run_store.list_runs(None, 20)?;
    let using_mock = runs.is_empty();

    let selected_run_id = if using_mock {
        choose_selected_run(&mock_runs(), selected)
    } else {
        choose_selected_run(&runs, selected)
    };

    Ok(RunsPageView {
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
            build_mock_run_detail(&selected_run_id)
        } else {
            build_live_run_detail(db, &selected_run_id)?
        },
    })
}

/// Load the detail panel for a single orchestration run.
pub fn load_run_detail(db: Arc<Database>, selected: Option<String>) -> Result<RunDetailView> {
    Ok(load_runs_page(db, selected)?.selected)
}

/// Load the agents page view-model, optionally selecting an agent by name.
pub fn load_agents_page(selected: Option<String>) -> Result<AgentsPageView> {
    let agents = load_profiles_catalog()?;
    let using_defaults = agents.iter().all(|profile| !profile.is_live);
    let selected_name = choose_selected_name(
        agents
            .iter()
            .map(|item| item.profile.title.clone())
            .collect(),
        selected,
    );

    Ok(AgentsPageView {
        mode_label: if using_defaults {
            "Bundled defaults".into()
        } else {
            "Installed catalog".into()
        },
        mode_tone: if using_defaults { "neutral" } else { "success" },
        agents: agents
            .iter()
            .map(|entry| AgentListItem {
                title: entry.profile.title.clone(),
                subtitle: entry
                    .profile
                    .description
                    .clone()
                    .unwrap_or_else(|| "No profile description provided.".into()),
                capability: capability_line(&entry.profile),
                source_label: entry.source_label.clone(),
                page_url: format!("/agents?agent={}", encode(&entry.profile.title)),
                active: entry.profile.title == selected_name,
            })
            .collect(),
        selected: build_agent_detail(
            agents
                .iter()
                .find(|entry| entry.profile.title == selected_name)
                .context("selected agent missing")?,
        )?,
    })
}

/// Load the detail panel for a single agent profile.
pub fn load_agent_detail(selected: Option<String>) -> Result<AgentDetailView> {
    Ok(load_agents_page(selected)?.selected)
}

/// Load the teams page view-model, optionally selecting a team by name.
pub fn load_teams_page(selected: Option<String>) -> Result<TeamsPageView> {
    let teams = load_teams_catalog()?;
    let using_defaults = teams.iter().all(|team| !team.is_live);
    let selected_name = choose_selected_name(
        teams.iter().map(|item| item.team.title.clone()).collect(),
        selected,
    );

    Ok(TeamsPageView {
        mode_label: if using_defaults {
            "Bundled defaults".into()
        } else {
            "Installed catalog".into()
        },
        mode_tone: if using_defaults { "neutral" } else { "success" },
        teams: teams
            .iter()
            .map(|entry| TeamListItem {
                title: entry.team.title.clone(),
                subtitle: entry
                    .team
                    .description
                    .clone()
                    .unwrap_or_else(|| "No team description provided.".into()),
                members: entry
                    .team
                    .agents
                    .iter()
                    .map(|agent| agent.profile.clone())
                    .collect::<Vec<_>>()
                    .join(" · "),
                source_label: entry.source_label.clone(),
                page_url: format!("/teams?team={}", encode(&entry.team.title)),
                active: entry.team.title == selected_name,
            })
            .collect(),
        selected: build_team_editor(
            teams
                .iter()
                .find(|entry| entry.team.title == selected_name)
                .context("selected team missing")?,
            None,
        )?,
    })
}

/// Load the YAML editor panel for a single team definition.
pub fn load_team_editor(selected: Option<String>) -> Result<TeamEditorView> {
    Ok(load_teams_page(selected)?.selected)
}

/// Load the workflows page view-model, optionally selecting a workflow by name.
pub fn load_workflows_page(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<WorkflowsPageView> {
    let teams = load_teams_catalog()?;
    let schedules = ScheduleStore::new(db.clone()).list()?;
    let triggers = TriggerStore::new(db.clone()).list()?;
    let recent_runs = OrchestrationStore::new(db).list_runs(None, 200)?;
    let using_preview = teams.iter().all(|team| !team.is_live)
        && schedules.is_empty()
        && triggers.is_empty()
        && recent_runs.is_empty();
    let selected_name = choose_selected_name(
        teams.iter().map(|item| item.name.clone()).collect(),
        selected,
    );
    let catalog = build_workflow_catalog(&teams, &schedules, &triggers, &recent_runs);

    Ok(WorkflowsPageView {
        mode_label: if using_preview {
            "Bundled defaults".into()
        } else {
            "Live registry".into()
        },
        mode_tone: if using_preview { "neutral" } else { "success" },
        workflows: catalog
            .iter()
            .map(|entry| build_workflow_list_item(entry, &selected_name))
            .collect(),
        selected: build_workflow_detail(
            catalog
                .iter()
                .find(|entry| entry.name == selected_name)
                .context("selected workflow missing")?,
        )?,
    })
}

/// Load the detail panel for a single workflow.
pub fn load_workflow_detail(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<WorkflowDetailView> {
    Ok(load_workflows_page(db, selected)?.selected)
}

/// Load the trigger management page view-model.
pub fn load_triggers_page(
    db: Arc<Database>,
    draft: Option<TriggerDraftView>,
    notice: Option<Notice>,
) -> Result<TriggersPageView> {
    let teams = load_teams_catalog()?;
    let triggers = TriggerStore::new(db).list()?;
    let default_workflow = teams
        .first()
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let default_trigger_type = TriggerType::all_names()
        .iter()
        .find(|name| **name == "on_message")
        .copied()
        .unwrap_or("on_message")
        .to_string();

    let form = draft.unwrap_or_else(|| {
        build_trigger_draft(
            String::new(),
            default_trigger_type.clone(),
            default_workflow.clone(),
            trigger_condition_example(&default_trigger_type).into(),
            "Triggered from the web dashboard".into(),
        )
    });

    let workflows: Vec<_> = teams
        .iter()
        .map(|entry| TriggerWorkflowOptionView {
            value: entry.name.clone(),
            label: entry.team.title.clone(),
            detail: entry.team.workflow_name(),
            selected: entry.name == form.workflow_name,
        })
        .collect();
    let trigger_types: Vec<_> = TriggerType::all_names()
        .iter()
        .map(|name| TriggerTypeOptionView {
            value: (*name).into(),
            label: humanize_trigger_type(name),
            selected: *name == form.trigger_type,
        })
        .collect();
    let covered_workflows = triggers
        .iter()
        .map(|trigger| trigger.team_name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let enabled_triggers = triggers.iter().filter(|trigger| trigger.enabled).count();
    let paused_triggers = triggers.len().saturating_sub(enabled_triggers);
    let trigger_types_count = triggers
        .iter()
        .map(|trigger| trigger.trigger_type.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    Ok(TriggersPageView {
        mode_label: if triggers.is_empty() {
            "Ready for setup".into()
        } else {
            "Live registry".into()
        },
        mode_tone: if triggers.is_empty() { "neutral" } else { "success" },
        notice,
        summary: vec![
            MetricCard {
                label: "Configured".into(),
                value: triggers.len().to_string(),
                note: "Saved triggers in the registry".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Enabled".into(),
                value: enabled_triggers.to_string(),
                note: format!("{paused_triggers} paused"),
                tone: "sage",
            },
            MetricCard {
                label: "Workflow coverage".into(),
                value: covered_workflows.to_string(),
                note: "Workflows with at least one trigger".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Trigger kinds".into(),
                value: trigger_types_count.to_string(),
                note: "Distinct trigger types configured".into(),
                tone: "neutral",
            },
        ],
        form,
        workflows,
        trigger_types,
        triggers: build_trigger_list_items(&teams, &triggers),
        empty_hint:
            "No triggers are configured yet. Create one to launch workflows from events or webhooks."
                .into(),
    })
}

/// Save edited team YAML and return the refreshed editor view.
pub fn save_team_yaml(original_name: String, yaml: String) -> Result<TeamEditorView> {
    let parsed = TeamDefinition::from_yaml(&yaml);
    match parsed {
        Ok(team) => {
            let store = TeamStore::new()?;
            if team.title != original_name
                && let Err(error) = store.remove(&original_name)
                && !error.to_string().contains("not found")
            {
                return Err(anyhow!(error));
            }
            store.save(&team, true)?;
            let entry = TeamCatalogEntry {
                name: team.name().to_string(),
                team,
                source_label: format!("Saved in {}", store.dir().display()),
                is_live: true,
            };
            build_team_editor(
                &entry,
                Some(Notice {
                    text: "Team definition saved.".into(),
                    tone: "success",
                }),
            )
        }
        Err(error) => Ok(TeamEditorView {
            title: original_name.clone(),
            subtitle: "Fix the YAML validation error and try again.".into(),
            source_label: "Editor draft".into(),
            workflow_label: "Unparsed".into(),
            members_text: "No members parsed".into(),
            original_name,
            yaml,
            notice: Some(Notice {
                text: error.to_string(),
                tone: "danger",
            }),
        }),
    }
}

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

/// Load the detail panel for a queue run's message traffic.
pub fn load_queue_detail(db: Arc<Database>, selected: Option<String>) -> Result<QueueDetailView> {
    Ok(load_queue_page(db, selected)?.selected)
}

#[derive(Clone)]
struct SessionRecord {
    summary: SessionSummary,
    messages: Vec<HistoryMessage>,
}

#[derive(Clone)]
struct ProfileCatalogEntry {
    profile: AgentProfile,
    source_label: String,
    is_live: bool,
}

#[derive(Clone)]
struct TeamCatalogEntry {
    name: String,
    team: TeamDefinition,
    source_label: String,
    is_live: bool,
}

#[derive(Clone)]
struct WorkflowCatalogEntry {
    name: String,
    team: TeamDefinition,
    source_label: String,
    schedules: Vec<Schedule>,
    triggers: Vec<Trigger>,
    recent_runs: Vec<OrchestrationRun>,
}

struct DurationStats {
    average_label: Option<String>,
    note: String,
}

fn live_sessions(store: &SessionStore, rows: &[SessionSummary]) -> Result<Vec<SessionRecord>> {
    rows.iter()
        .map(|summary| {
            let key = SessionKey::from_stable_id(&summary.session_key);
            Ok(SessionRecord {
                summary: summary.clone(),
                messages: store.load_history(&key, 40)?,
            })
        })
        .collect()
}

fn build_dashboard_activities(
    db: Arc<Database>,
    runs: &[OrchestrationRun],
    sessions: &[SessionRecord],
    queue_stats: &QueueStats,
    using_mock: bool,
) -> Result<Vec<ActivityItem>> {
    let store = AgentMessageStore::new(db);
    let messages = store.list_recent_global(8)?;
    if !messages.is_empty() {
        return Ok(messages
            .into_iter()
            .map(|message| {
                let meta = activity_meta(&message);
                let detail = preview(&message.payload, 132);
                let tone = message_tone(&message.status);
                ActivityItem {
                    actor: message.from_agent,
                    meta,
                    detail,
                    timestamp: message.created_at,
                    tone,
                }
            })
            .collect());
    }

    if using_mock {
        return Ok(mock_dashboard_activities());
    }

    Ok(synthetic_dashboard_activities(runs, sessions, queue_stats))
}

fn activity_meta(message: &AgentMessage) -> String {
    if let Some(target) = &message.to_agent {
        format!(
            "Directed to {target} · {} · {}",
            message.session_key,
            message.status.as_str()
        )
    } else if let Some(channel) = &message.channel {
        format!(
            "Published to #{channel} · {} · {}",
            message.session_key,
            message.status.as_str()
        )
    } else {
        format!("{} · {}", message.session_key, message.status.as_str())
    }
}

fn mock_dashboard_activities() -> Vec<ActivityItem> {
    vec![
        ActivityItem {
            actor: "architect".into(),
            meta: "Published to #ops · discord:ns:studio-a:ops-bridge · delivered".into(),
            detail: "Signal board framing approved. Keep the shell resilient, then layer live fragments through SSE.".into(),
            timestamp: "2026-03-10 10:29".into(),
            tone: "cyan",
        },
        ActivityItem {
            actor: "developer".into(),
            meta: "Directed to reviewer · discord:ns:studio-a:ops-bridge · pending".into(),
            detail: "Handing off the live dashboard layout and queue telemetry for review.".into(),
            timestamp: "2026-03-10 10:23".into(),
            tone: "amber",
        },
        ActivityItem {
            actor: "reviewer".into(),
            meta: "Directed to developer · telegram:direct:founder-42 · acknowledged".into(),
            detail: "One migration note is still missing, otherwise the monitoring pass is ready.".into(),
            timestamp: "2026-03-10 09:44".into(),
            tone: "sage",
        },
    ]
}

fn synthetic_dashboard_activities(
    runs: &[OrchestrationRun],
    sessions: &[SessionRecord],
    queue_stats: &QueueStats,
) -> Vec<ActivityItem> {
    let mut items: Vec<ActivityItem> = runs
        .iter()
        .take(4)
        .map(|run| ActivityItem {
            actor: run.team_name.clone(),
            meta: format!("{} · {}", run.status.as_str(), progress_label(run)),
            detail: preview(&run.input, 132),
            timestamp: run.updated_at.clone(),
            tone: run_tone(&run.status),
        })
        .collect();

    items.extend(
        sessions
            .iter()
            .filter_map(|session| session.messages.last().map(|message| (session, message)))
            .take(2)
            .map(|(session, message)| ActivityItem {
                actor: session.summary.session_key.clone(),
                meta: format!(
                    "{} · {}",
                    session
                        .summary
                        .active_team
                        .clone()
                        .unwrap_or_else(|| "no active team".into()),
                    message.role
                ),
                detail: preview(&message.content, 132),
                timestamp: message.created_at.clone(),
                tone: if message.role == "assistant" {
                    "cyan"
                } else {
                    "neutral"
                },
            }),
    );

    if queue_stats.dead > 0 {
        items.push(ActivityItem {
            actor: "queue-monitor".into(),
            meta: "dead letters detected".into(),
            detail: format!(
                "{} item(s) require manual intervention before they can be retried.",
                queue_stats.dead
            ),
            timestamp: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            tone: "rose",
        });
    }

    items.truncate(6);
    items
}

fn duration_stats(runs: &[OrchestrationRun]) -> DurationStats {
    let durations: Vec<i64> = runs.iter().filter_map(run_duration_seconds).collect();
    if durations.is_empty() {
        return DurationStats {
            average_label: None,
            note: "Run duration will appear once persisted timestamps accumulate.".into(),
        };
    }

    let average = durations.iter().sum::<i64>() / durations.len() as i64;
    let max = durations.iter().copied().max().unwrap_or(average);
    DurationStats {
        average_label: Some(format_duration(average)),
        note: format!(
            "{} captured runs · longest {}",
            durations.len(),
            format_duration(max)
        ),
    }
}

fn build_status_segments(segments: Vec<(&str, i64, &'static str)>) -> Vec<StatusSegment> {
    let segment_count = segments.len().max(1) as u8;
    let total = segments.iter().map(|(_, value, _)| *value).sum::<i64>();
    segments
        .into_iter()
        .filter(|(_, value, _)| *value > 0 || total == 0)
        .map(|(label, value, tone)| StatusSegment {
            label: label.into(),
            value: value.to_string(),
            tone,
            width: if total == 0 {
                100 / segment_count
            } else {
                ((value as f32 / total as f32) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8
            },
        })
        .collect()
}

fn build_duration_bars(runs: &[OrchestrationRun]) -> Vec<TrendBar> {
    let durations: Vec<(&OrchestrationRun, i64)> = runs
        .iter()
        .take(6)
        .filter_map(|run| run_duration_seconds(run).map(|duration| (run, duration)))
        .collect();
    let max = durations
        .iter()
        .map(|(_, duration)| *duration)
        .max()
        .unwrap_or(0);

    durations
        .into_iter()
        .map(|(run, duration)| TrendBar {
            label: preview(&run.team_name, 18),
            value: format_duration(duration),
            detail: run.status.as_str().into(),
            tone: run_tone(&run.status),
            height: if max == 0 {
                34
            } else {
                ((duration as f32 / max as f32) * 100.0)
                    .round()
                    .clamp(24.0, 100.0) as u8
            },
        })
        .collect()
}

fn run_duration_seconds(run: &OrchestrationRun) -> Option<i64> {
    let started = parse_timestamp(&run.created_at)?;
    let finished = match run.status {
        RunStatus::Running => Utc::now().naive_utc(),
        RunStatus::Completed | RunStatus::Failed | RunStatus::Suspended => {
            parse_timestamp(&run.updated_at)?
        }
    };

    let duration = finished.signed_duration_since(started).num_seconds();
    Some(duration.max(0))
}

fn parse_timestamp(value: &str) -> Option<NaiveDateTime> {
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn ratio_percent(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        0
    } else {
        ((numerator as f32 / denominator as f32) * 100.0).round() as usize
    }
}

fn message_tone(status: &AgentMessageStatus) -> &'static str {
    match status {
        AgentMessageStatus::Pending => "amber",
        AgentMessageStatus::Delivered => "cyan",
        AgentMessageStatus::Acknowledged => "sage",
    }
}

fn mock_sessions() -> Vec<SessionRecord> {
    vec![
        SessionRecord {
            summary: SessionSummary {
                session_key: "discord:ns:studio-a:ops-bridge".into(),
                active_team: Some("feature-dev".into()),
                created_at: "2026-03-10 09:00".into(),
                updated_at: "2026-03-10 10:28".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Spin up a reviewer and confirm the deploy checklist.".into(),
                    author: Some("pm-sora".into()),
                    created_at: "2026-03-10 10:11".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content: "Feature-dev is active. Routing implementation notes to reviewer next.".into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:12".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content: "Reviewer flagged one missing migration note. Queue updated for follow-up.".into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:28".into(),
                },
            ],
        },
        SessionRecord {
            summary: SessionSummary {
                session_key: "telegram:direct:founder-42".into(),
                active_team: None,
                created_at: "2026-03-10 08:21".into(),
                updated_at: "2026-03-10 09:44".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Summarize the backlog movement from this morning.".into(),
                    author: Some("founder".into()),
                    created_at: "2026-03-10 09:40".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content: "Three frontend issues advanced to implementation, one queue alert remains unresolved.".into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 09:44".into(),
                },
            ],
        },
    ]
}

fn mock_runs() -> Vec<OrchestrationRun> {
    vec![
        OrchestrationRun {
            team_run_id: "run-preview-01".into(),
            session_key: "discord:ns:studio-a:ops-bridge".into(),
            team_name: "feature-dev".into(),
            workflow: "chain".into(),
            input: "Implement the live dashboard shell and verify the orchestration views.".into(),
            status: RunStatus::Running,
            current_step: 2,
            total_steps: 4,
            result: None,
            created_at: "2026-03-10 10:02".into(),
            updated_at: "2026-03-10 10:29".into(),
        },
        OrchestrationRun {
            team_run_id: "run-preview-02".into(),
            session_key: "discord:ns:studio-a:ops-bridge".into(),
            team_name: "research-panel".into(),
            workflow: "fan_out".into(),
            input: "Compare provider latency across three channels.".into(),
            status: RunStatus::Completed,
            current_step: 3,
            total_steps: 3,
            result: Some("Discord remains fastest for burst replies; Telegram is most stable under edit throttling.".into()),
            created_at: "2026-03-10 08:15".into(),
            updated_at: "2026-03-10 08:33".into(),
        },
        OrchestrationRun {
            team_run_id: "run-preview-03".into(),
            session_key: "telegram:direct:founder-42".into(),
            team_name: "smart-router".into(),
            workflow: "router".into(),
            input: "Route an incoming request to the correct specialist.".into(),
            status: RunStatus::Suspended,
            current_step: 1,
            total_steps: 2,
            result: Some("Waiting on an external credential refresh before resuming.".into()),
            created_at: "2026-03-10 07:58".into(),
            updated_at: "2026-03-10 08:05".into(),
        },
    ]
}

fn build_session_list_items(
    sessions: &[SessionRecord],
    selected_key: Option<String>,
    source_label: &str,
) -> Vec<SessionListItem> {
    sessions
        .iter()
        .map(|session| {
            let parsed = SessionKey::from_stable_id(&session.summary.session_key);
            let title = match &parsed.namespace {
                Some(namespace) => format!("{namespace} / {}", parsed.channel_id),
                None => parsed.channel_id.clone(),
            };
            let subtitle = session
                .summary
                .active_team
                .clone()
                .map(|team| format!("{} active · {}", team, source_label))
                .unwrap_or_else(|| format!("No active team · {source_label}"));
            let preview = session
                .messages
                .last()
                .map(|message| preview(&message.content, 84))
                .unwrap_or_else(|| "No messages captured yet.".into());
            let encoded = encode(&session.summary.session_key);
            SessionListItem {
                title,
                subtitle,
                preview,
                updated_at: session.summary.updated_at.clone(),
                badge: parsed.platform.as_str().to_uppercase(),
                badge_tone: platform_tone(parsed.platform.as_str()),
                page_url: format!("/sessions?session={encoded}"),
                active: selected_key
                    .as_ref()
                    .map(|key| key == &session.summary.session_key)
                    .unwrap_or(false),
            }
        })
        .collect()
}

fn build_session_detail(session: &SessionRecord, source_label: &str) -> SessionDetailView {
    let parsed = SessionKey::from_stable_id(&session.summary.session_key);
    SessionDetailView {
        title: format!("Session {}", parsed.channel_id),
        subtitle: match &parsed.namespace {
            Some(namespace) => format!("{} / {}", parsed.platform.as_str(), namespace),
            None => format!("{} / direct", parsed.platform.as_str()),
        },
        source_label: source_label.into(),
        meta: vec![
            MetaRow {
                label: "Stable key".into(),
                value: session.summary.session_key.clone(),
            },
            MetaRow {
                label: "Active team".into(),
                value: session
                    .summary
                    .active_team
                    .clone()
                    .unwrap_or_else(|| "None".into()),
            },
            MetaRow {
                label: "Created".into(),
                value: session.summary.created_at.clone(),
            },
            MetaRow {
                label: "Last update".into(),
                value: session.summary.updated_at.clone(),
            },
        ],
        messages: session
            .messages
            .iter()
            .map(|message| MessageBubble {
                role_label: if message.role == "assistant" {
                    "Assistant".into()
                } else {
                    "User".into()
                },
                author_label: message.author.clone().unwrap_or_else(|| "unknown".into()),
                timestamp: message.created_at.clone(),
                content: message.content.clone(),
                tone: if message.role == "assistant" {
                    "accent"
                } else {
                    "plain"
                },
                alignment: if message.role == "assistant" {
                    "right"
                } else {
                    "left"
                },
            })
            .collect(),
        empty_hint: "This session has no persisted messages yet.".into(),
    }
}

fn choose_selected_session(sessions: &[SessionRecord], selected: Option<String>) -> String {
    selected
        .filter(|target| {
            sessions
                .iter()
                .any(|session| session.summary.session_key == *target)
        })
        .unwrap_or_else(|| sessions[0].summary.session_key.clone())
}

fn build_run_list_items(
    runs: &[OrchestrationRun],
    selected_run_id: Option<String>,
    source_label: &str,
) -> Vec<RunListItem> {
    runs.iter()
        .map(|run| RunListItem {
            title: run.team_name.clone(),
            subtitle: format!("{} workflow · {}", run.workflow, source_label),
            updated_at: run.updated_at.clone(),
            progress_label: progress_label(run),
            badge: run.status.as_str().to_uppercase(),
            badge_tone: run_tone(&run.status),
            page_url: format!("/runs?run={}", encode(&run.team_run_id)),
            queue_page_url: format!("/queue?run={}", encode(&run.team_run_id)),
            active: selected_run_id
                .as_ref()
                .map(|selected| selected == &run.team_run_id)
                .unwrap_or(false),
        })
        .collect()
}

fn build_live_run_detail(db: Arc<Database>, run_id: &str) -> Result<RunDetailView> {
    let run_store = OrchestrationStore::new(db.clone());
    let work_store = WorkItemStore::new(db.clone());
    let queue = MessageQueue::new(db);

    let run = run_store
        .get_run(run_id)?
        .with_context(|| format!("run `{run_id}` not found"))?;
    let work_items = work_store.list_for_run(run_id, None)?;
    let broadcasts = queue.read_broadcasts(run_id, None)?;

    Ok(build_run_detail(
        &run,
        &work_items,
        &broadcasts,
        "Live runtime",
    ))
}

fn build_run_detail(
    run: &OrchestrationRun,
    work_items: &[WorkItem],
    broadcasts: &[QueueMessage],
    source_label: &str,
) -> RunDetailView {
    RunDetailView {
        title: format!("Run {}", run.team_run_id),
        subtitle: format!("{} / {}", run.team_name, run.workflow),
        source_label: source_label.into(),
        meta: vec![
            MetaRow {
                label: "Status".into(),
                value: run.status.as_str().into(),
            },
            MetaRow {
                label: "Progress".into(),
                value: progress_label(run),
            },
            MetaRow {
                label: "Session".into(),
                value: run.session_key.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: run.updated_at.clone(),
            },
        ],
        work_items: work_items
            .iter()
            .map(|item| WorkItemView {
                title: item.title.clone(),
                detail: item
                    .assigned_to
                    .clone()
                    .map(|assignee| format!("{assignee} · {}", item.updated_at))
                    .unwrap_or_else(|| item.updated_at.clone()),
                status_label: item.status.as_str().replace('_', " "),
                status_tone: work_tone(&item.status),
                step_label: item
                    .workflow_step
                    .map(|step| format!("Step {step}"))
                    .unwrap_or_else(|| "Root item".into()),
                indent_class: if item.parent_id.is_some() {
                    "is-child"
                } else {
                    "is-root"
                },
            })
            .collect(),
        broadcasts: broadcasts
            .iter()
            .map(|message| BroadcastView {
                sender: message.sender.clone(),
                created_at: message.created_at.clone(),
                content: message.content.clone(),
            })
            .collect(),
        input: run.input.clone(),
        result: run
            .result
            .clone()
            .unwrap_or_else(|| "No final result has been recorded yet.".into()),
        empty_hint: "No work items or broadcasts have been captured for this run yet.".into(),
    }
}

fn build_mock_run_detail(run_id: &str) -> RunDetailView {
    let runs = mock_runs();
    let run = runs
        .iter()
        .find(|run| run.team_run_id == run_id)
        .unwrap_or(&runs[0]);
    let work_items = vec![
        WorkItem {
            id: 1,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            parent_id: None,
            title: "Frame the dashboard information architecture".into(),
            description: None,
            status: WorkStatus::Completed,
            assigned_to: Some("architect".into()),
            workflow_step: Some(0),
            input: None,
            output: None,
            error: None,
            created_at: run.created_at.clone(),
            updated_at: run.updated_at.clone(),
        },
        WorkItem {
            id: 2,
            session_key: run.session_key.clone(),
            team_run_id: run.team_run_id.clone(),
            parent_id: Some(1),
            title: "Implement Askama shell and HTMX detail panes".into(),
            description: None,
            status: WorkStatus::InProgress,
            assigned_to: Some("developer".into()),
            workflow_step: Some(1),
            input: None,
            output: None,
            error: None,
            created_at: run.created_at.clone(),
            updated_at: run.updated_at.clone(),
        },
    ];
    let broadcasts = vec![QueueMessage {
        id: 11,
        session_key: run.session_key.clone(),
        team_run_id: run.team_run_id.clone(),
        sender: "architect".into(),
        recipient: "broadcast".into(),
        content:
            "Signal-first layout approved. Proceed with the operations board visual direction."
                .into(),
        msg_type: opengoose_persistence::MessageType::Broadcast,
        status: opengoose_persistence::MessageStatus::Completed,
        retry_count: 0,
        max_retries: 3,
        created_at: run.updated_at.clone(),
        processed_at: None,
        error: None,
    }];
    build_run_detail(run, &work_items, &broadcasts, "Mock preview")
}

fn choose_selected_run(runs: &[OrchestrationRun], selected: Option<String>) -> String {
    selected
        .filter(|target| runs.iter().any(|run| run.team_run_id == *target))
        .unwrap_or_else(|| runs[0].team_run_id.clone())
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

fn load_profiles_catalog() -> Result<Vec<ProfileCatalogEntry>> {
    let store = ProfileStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_profiles()
            .into_iter()
            .map(|profile| ProfileCatalogEntry {
                profile,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let profile = store.get(&name)?;
            Ok(ProfileCatalogEntry {
                profile,
                source_label: store.profile_path(&name),
                is_live: true,
            })
        })
        .collect()
}

fn build_agent_detail(entry: &ProfileCatalogEntry) -> Result<AgentDetailView> {
    let settings = profile_settings(&entry.profile);
    let extensions = entry
        .profile
        .extensions
        .iter()
        .map(|extension| ExtensionRow {
            name: extension.name.clone(),
            kind: extension.ext_type.clone(),
            summary: extension
                .cmd
                .clone()
                .or_else(|| extension.uri.clone())
                .or_else(|| extension.code.as_ref().map(|_| "inline python".into()))
                .unwrap_or_else(|| "No runtime configuration".into()),
        })
        .collect();

    Ok(AgentDetailView {
        title: entry.profile.title.clone(),
        subtitle: entry
            .profile
            .description
            .clone()
            .unwrap_or_else(|| "No profile description provided.".into()),
        source_label: entry.source_label.clone(),
        instructions_preview: preview(
            entry
                .profile
                .instructions
                .as_deref()
                .or(entry.profile.prompt.as_deref())
                .unwrap_or("No instructions or prompt configured."),
            420,
        ),
        settings,
        activities: entry.profile.activities.clone().unwrap_or_default(),
        skills: entry.profile.skills.clone(),
        extensions,
        yaml: entry.profile.to_yaml()?,
    })
}

fn capability_line(profile: &AgentProfile) -> String {
    let provider = profile
        .settings
        .as_ref()
        .and_then(|settings| settings.goose_provider.clone())
        .unwrap_or_else(|| "provider unset".into());
    let model = profile
        .settings
        .as_ref()
        .and_then(|settings| settings.goose_model.clone())
        .unwrap_or_else(|| "model unset".into());
    format!("{provider} / {model}")
}

fn profile_settings(profile: &AgentProfile) -> Vec<SettingRow> {
    let mut rows = Vec::new();
    if let Some(settings) = &profile.settings {
        if let Some(provider) = &settings.goose_provider {
            rows.push(SettingRow {
                label: "Provider".into(),
                value: provider.clone(),
            });
        }
        if let Some(model) = &settings.goose_model {
            rows.push(SettingRow {
                label: "Model".into(),
                value: model.clone(),
            });
        }
        if let Some(temperature) = settings.temperature {
            rows.push(SettingRow {
                label: "Temperature".into(),
                value: temperature.to_string(),
            });
        }
        if let Some(max_turns) = settings.max_turns {
            rows.push(SettingRow {
                label: "Max turns".into(),
                value: max_turns.to_string(),
            });
        }
        if let Some(max_retries) = settings.max_retries {
            rows.push(SettingRow {
                label: "Retries".into(),
                value: max_retries.to_string(),
            });
        }
    }
    if rows.is_empty() {
        rows.push(SettingRow {
            label: "Settings".into(),
            value: "No explicit settings block".into(),
        });
    }
    rows
}

fn load_teams_catalog() -> Result<Vec<TeamCatalogEntry>> {
    let store = TeamStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_teams()
            .into_iter()
            .map(|team| TeamCatalogEntry {
                name: team.name().to_string(),
                team,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let team = store.get(&name)?;
            Ok(TeamCatalogEntry {
                name,
                team,
                source_label: format!("{}", store.dir().display()),
                is_live: true,
            })
        })
        .collect()
}

fn build_workflow_catalog(
    teams: &[TeamCatalogEntry],
    schedules: &[Schedule],
    triggers: &[Trigger],
    recent_runs: &[OrchestrationRun],
) -> Vec<WorkflowCatalogEntry> {
    teams
        .iter()
        .map(|entry| WorkflowCatalogEntry {
            name: entry.name.clone(),
            team: entry.team.clone(),
            source_label: entry.source_label.clone(),
            schedules: schedules
                .iter()
                .filter(|schedule| schedule.team_name == entry.name)
                .cloned()
                .collect(),
            triggers: triggers
                .iter()
                .filter(|trigger| trigger.team_name == entry.name)
                .cloned()
                .collect(),
            recent_runs: recent_runs
                .iter()
                .filter(|run| run.team_name == entry.name)
                .take(6)
                .cloned()
                .collect(),
        })
        .collect()
}

fn build_team_editor(entry: &TeamCatalogEntry, notice: Option<Notice>) -> Result<TeamEditorView> {
    Ok(TeamEditorView {
        title: entry.team.title.clone(),
        subtitle: entry
            .team
            .description
            .clone()
            .unwrap_or_else(|| "No team description provided.".into()),
        source_label: entry.source_label.clone(),
        workflow_label: entry.team.workflow_name(),
        members_text: entry
            .team
            .agents
            .iter()
            .map(|agent| agent.profile.clone())
            .collect::<Vec<_>>()
            .join(", "),
        original_name: entry.team.title.clone(),
        yaml: entry.team.to_yaml()?,
        notice,
    })
}

fn build_workflow_list_item(entry: &WorkflowCatalogEntry, selected_name: &str) -> WorkflowListItem {
    let (workflow_status_label, workflow_status_tone) = workflow_status(entry);
    WorkflowListItem {
        title: entry.team.title.clone(),
        subtitle: entry
            .team
            .description
            .clone()
            .unwrap_or_else(|| format!("{} workflow", entry.team.workflow_name())),
        preview: format!(
            "{} · {}",
            automation_summary(entry),
            team_agent_summary(&entry.team)
        ),
        source_label: entry.source_label.clone(),
        status_label: workflow_status_label,
        status_tone: workflow_status_tone,
        page_url: format!("/workflows?workflow={}", encode(&entry.name)),
        active: entry.name == selected_name,
    }
}

fn build_workflow_detail(entry: &WorkflowCatalogEntry) -> Result<WorkflowDetailView> {
    let (workflow_status_label, workflow_status_tone) = workflow_status(entry);
    let last_run = entry.recent_runs.first();
    Ok(WorkflowDetailView {
        title: entry.team.title.clone(),
        subtitle: entry
            .team
            .description
            .clone()
            .unwrap_or_else(|| "No workflow description provided.".into()),
        source_label: entry.source_label.clone(),
        status_label: workflow_status_label,
        status_tone: workflow_status_tone,
        meta: vec![
            MetaRow {
                label: "Pattern".into(),
                value: entry.team.workflow_name(),
            },
            MetaRow {
                label: "Agents".into(),
                value: entry.team.agents.len().to_string(),
            },
            MetaRow {
                label: "Schedules".into(),
                value: enabled_total_label(
                    entry
                        .schedules
                        .iter()
                        .filter(|schedule| schedule.enabled)
                        .count(),
                    entry.schedules.len(),
                ),
            },
            MetaRow {
                label: "Triggers".into(),
                value: enabled_total_label(
                    entry
                        .triggers
                        .iter()
                        .filter(|trigger| trigger.enabled)
                        .count(),
                    entry.triggers.len(),
                ),
            },
            MetaRow {
                label: "Last run".into(),
                value: last_run
                    .map(|run| {
                        format!(
                            "{} · {}",
                            display_status_label(run.status.as_str()),
                            run.updated_at
                        )
                    })
                    .unwrap_or_else(|| "No persisted runs yet.".into()),
            },
            MetaRow {
                label: "Automation".into(),
                value: automation_summary(entry),
            },
        ],
        steps: entry
            .team
            .agents
            .iter()
            .enumerate()
            .map(|(index, agent)| WorkflowStepView {
                title: format!(
                    "{} · {}",
                    step_prefix(&entry.team.workflow, index),
                    agent.profile
                ),
                detail: agent
                    .role
                    .clone()
                    .unwrap_or_else(|| "No role description provided.".into()),
                badge: step_badge(&entry.team.workflow).into(),
                badge_tone: step_badge_tone(&entry.team.workflow),
            })
            .collect(),
        automations: build_workflow_automations(entry),
        recent_runs: entry
            .recent_runs
            .iter()
            .map(|run| WorkflowRunView {
                title: format!("Run {}", run.team_run_id),
                detail: format!(
                    "{} · {}",
                    progress_label(run),
                    run.result
                        .as_deref()
                        .map(|result| preview(result, 72))
                        .unwrap_or_else(|| "Still executing".into())
                ),
                updated_at: run.updated_at.clone(),
                status_label: display_status_label(run.status.as_str()),
                status_tone: run_tone(&run.status),
                page_url: format!("/runs?run={}", encode(&run.team_run_id)),
            })
            .collect(),
        yaml: entry.team.to_yaml()?,
        trigger_api_url: format!("/api/workflows/{}/trigger", encode(&entry.name)),
        manage_triggers_url: "/triggers".into(),
        trigger_input: format!(
            "Manual run requested from the web dashboard for {}",
            entry.name
        ),
    })
}

fn build_workflow_automations(entry: &WorkflowCatalogEntry) -> Vec<WorkflowAutomationView> {
    let schedules = entry
        .schedules
        .iter()
        .map(|schedule| WorkflowAutomationView {
            kind: "Schedule".into(),
            title: schedule.name.clone(),
            detail: format!("{} · team {}", schedule.cron_expression, schedule.team_name),
            note: match (&schedule.last_run_at, &schedule.next_run_at) {
                (Some(last_run), Some(next_run)) => format!("Last {last_run} · Next {next_run}"),
                (Some(last_run), None) => format!("Last {last_run}"),
                (None, Some(next_run)) => format!("Next {next_run}"),
                (None, None) => "No executions recorded yet.".into(),
            },
            status_label: if schedule.enabled {
                "Enabled".into()
            } else {
                "Paused".into()
            },
            status_tone: if schedule.enabled { "sage" } else { "neutral" },
        });
    let triggers = entry.triggers.iter().map(|trigger| WorkflowAutomationView {
        kind: "Trigger".into(),
        title: trigger.name.clone(),
        detail: format!(
            "{} · {}",
            trigger.trigger_type.replace('_', " "),
            preview(&trigger.condition_json, 72)
        ),
        note: trigger
            .last_fired_at
            .as_ref()
            .map(|last_fired| {
                format!(
                    "Last fired {last_fired} · {} total fire(s)",
                    trigger.fire_count
                )
            })
            .unwrap_or_else(|| format!("{} total fire(s)", trigger.fire_count)),
        status_label: if trigger.enabled {
            "Enabled".into()
        } else {
            "Paused".into()
        },
        status_tone: if trigger.enabled { "sage" } else { "neutral" },
    });

    schedules.chain(triggers).collect()
}

pub fn build_trigger_draft(
    name: String,
    trigger_type: String,
    workflow_name: String,
    condition_json: String,
    input: String,
) -> TriggerDraftView {
    let selected_type = if TriggerType::parse(&trigger_type).is_some() {
        trigger_type
    } else {
        "on_message".into()
    };
    let normalized_condition = if condition_json.trim().is_empty() {
        trigger_condition_example(&selected_type).into()
    } else {
        condition_json
    };

    TriggerDraftView {
        name,
        trigger_type: selected_type.clone(),
        workflow_name,
        condition_json: normalized_condition,
        condition_help: format!(
            "Provide a JSON object. Example: {}",
            trigger_condition_example(&selected_type)
        ),
        input,
    }
}

fn build_trigger_list_items(
    teams: &[TeamCatalogEntry],
    triggers: &[Trigger],
) -> Vec<TriggerListItemView> {
    triggers
        .iter()
        .map(|trigger| {
            let workflow_title = teams
                .iter()
                .find(|entry| entry.name == trigger.team_name)
                .map(|entry| entry.team.title.clone())
                .unwrap_or_else(|| trigger.team_name.clone());
            TriggerListItemView {
                name: trigger.name.clone(),
                trigger_type: trigger.trigger_type.clone(),
                trigger_type_label: humanize_trigger_type(&trigger.trigger_type),
                workflow_title,
                workflow_page_url: format!("/workflows?workflow={}", encode(&trigger.team_name)),
                status_label: if trigger.enabled {
                    "Enabled".into()
                } else {
                    "Paused".into()
                },
                status_tone: if trigger.enabled { "sage" } else { "neutral" },
                fire_count: trigger.fire_count,
                last_fired_at: trigger
                    .last_fired_at
                    .clone()
                    .unwrap_or_else(|| "Never".into()),
                condition_preview: preview(&trigger.condition_json, 120),
                input_preview: if trigger.input.trim().is_empty() {
                    "No custom workflow input.".into()
                } else {
                    preview(&trigger.input, 120)
                },
                toggle_label: if trigger.enabled {
                    "Pause".into()
                } else {
                    "Enable".into()
                },
                toggle_enabled_value: !trigger.enabled,
                search_text: format!(
                    "{} {} {} {} {}",
                    trigger.name,
                    trigger.trigger_type,
                    trigger.team_name,
                    trigger.condition_json,
                    trigger.input
                ),
            }
        })
        .collect()
}

fn trigger_condition_example(trigger_type: &str) -> &'static str {
    match trigger_type {
        "file_watch" => r#"{"pattern":"src/**/*.rs"}"#,
        "message_received" => r#"{"from_agent":"planner","payload_contains":"ship it"}"#,
        "schedule_complete" => r#"{"schedule_name":"nightly-review"}"#,
        "webhook_received" => r#"{"path":"/github/pr"}"#,
        "on_message" => r#"{"content_contains":"deploy"}"#,
        "on_session_start" | "on_session_end" => r#"{"platform":"discord"}"#,
        "on_schedule" => r#"{"team":"feature-dev"}"#,
        _ => "{}",
    }
}

fn humanize_trigger_type(raw: &str) -> String {
    raw.split('_')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn choose_selected_name(options: Vec<String>, selected: Option<String>) -> String {
    selected
        .filter(|target| options.iter().any(|candidate| candidate == target))
        .unwrap_or_else(|| options[0].clone())
}

fn queue_total(stats: &QueueStats) -> i64 {
    stats.pending + stats.processing + stats.completed + stats.failed + stats.dead
}

fn progress_label(run: &OrchestrationRun) -> String {
    format!("{}/{} steps", run.current_step, run.total_steps)
}

fn preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn platform_tone(platform: &str) -> &'static str {
    match platform {
        "discord" => "cyan",
        "telegram" => "sage",
        "slack" => "amber",
        _ => "neutral",
    }
}

fn run_tone(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "cyan",
        RunStatus::Completed => "sage",
        RunStatus::Failed => "rose",
        RunStatus::Suspended => "amber",
    }
}

fn work_tone(status: &WorkStatus) -> &'static str {
    match status {
        WorkStatus::Pending => "amber",
        WorkStatus::InProgress => "cyan",
        WorkStatus::Completed => "sage",
        WorkStatus::Failed => "rose",
        WorkStatus::Cancelled => "neutral",
    }
}

fn queue_tone(status: &opengoose_persistence::MessageStatus) -> &'static str {
    match status {
        opengoose_persistence::MessageStatus::Pending => "amber",
        opengoose_persistence::MessageStatus::Processing => "cyan",
        opengoose_persistence::MessageStatus::Completed => "sage",
        opengoose_persistence::MessageStatus::Failed => "rose",
        opengoose_persistence::MessageStatus::Dead => "rose",
    }
}

trait WorkflowName {
    fn workflow_name(&self) -> String;
}

impl WorkflowName for TeamDefinition {
    fn workflow_name(&self) -> String {
        match self.workflow {
            opengoose_teams::OrchestrationPattern::Chain => "Chain".into(),
            opengoose_teams::OrchestrationPattern::FanOut => "Fan-out".into(),
            opengoose_teams::OrchestrationPattern::Router => "Router".into(),
        }
    }
}

fn workflow_status(entry: &WorkflowCatalogEntry) -> (String, &'static str) {
    if let Some(run) = entry.recent_runs.first() {
        return (
            display_status_label(run.status.as_str()),
            run_tone(&run.status),
        );
    }

    if entry.schedules.iter().any(|schedule| schedule.enabled)
        || entry.triggers.iter().any(|trigger| trigger.enabled)
    {
        return ("Armed".into(), "amber");
    }

    ("Manual".into(), "neutral")
}

fn automation_summary(entry: &WorkflowCatalogEntry) -> String {
    let enabled_schedules = entry
        .schedules
        .iter()
        .filter(|schedule| schedule.enabled)
        .count();
    let enabled_triggers = entry
        .triggers
        .iter()
        .filter(|trigger| trigger.enabled)
        .count();

    match (entry.schedules.len(), entry.triggers.len()) {
        (0, 0) => "Manual only".into(),
        _ => format!(
            "{} · {}",
            enabled_total_label(enabled_schedules, entry.schedules.len()),
            enabled_total_label(enabled_triggers, entry.triggers.len()),
        ),
    }
}

fn team_agent_summary(team: &TeamDefinition) -> String {
    team.agents
        .iter()
        .map(|agent| agent.profile.clone())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn enabled_total_label(enabled: usize, total: usize) -> String {
    if total == 0 {
        "0 configured".into()
    } else {
        format!("{enabled}/{total} enabled")
    }
}

fn display_status_label(value: &str) -> String {
    value
        .split('_')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut label = first.to_uppercase().collect::<String>();
                    label.push_str(chars.as_str());
                    label
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn step_prefix(pattern: &opengoose_teams::OrchestrationPattern, index: usize) -> String {
    match pattern {
        opengoose_teams::OrchestrationPattern::Chain => format!("Step {}", index + 1),
        opengoose_teams::OrchestrationPattern::FanOut => format!("Branch {}", index + 1),
        opengoose_teams::OrchestrationPattern::Router => format!("Route {}", index + 1),
    }
}

fn step_badge(pattern: &opengoose_teams::OrchestrationPattern) -> &'static str {
    match pattern {
        opengoose_teams::OrchestrationPattern::Chain => "Sequential",
        opengoose_teams::OrchestrationPattern::FanOut => "Parallel",
        opengoose_teams::OrchestrationPattern::Router => "Candidate",
    }
}

fn step_badge_tone(pattern: &opengoose_teams::OrchestrationPattern) -> &'static str {
    match pattern {
        opengoose_teams::OrchestrationPattern::Chain => "cyan",
        opengoose_teams::OrchestrationPattern::FanOut => "amber",
        opengoose_teams::OrchestrationPattern::Router => "sage",
    }
}

#[cfg(test)]
mod tests {
    use opengoose_persistence::{MessageStatus, MessageType};
    use opengoose_profiles::{ExtensionRef, ProfileSettings};
    use opengoose_teams::OrchestrationPattern;

    use super::*;

    fn sample_run(
        team_run_id: &str,
        status: RunStatus,
        created_at: &str,
        updated_at: &str,
    ) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: team_run_id.into(),
            session_key: "discord:test:ops".into(),
            team_name: format!("team-{team_run_id}"),
            workflow: "chain".into(),
            input: "Investigate the live dashboard state".into(),
            status,
            current_step: 1,
            total_steps: 3,
            result: None,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        }
    }

    fn sample_session(session_key: &str, active_team: Option<&str>) -> SessionRecord {
        SessionRecord {
            summary: SessionSummary {
                session_key: session_key.into(),
                active_team: active_team.map(str::to_string),
                created_at: "2026-03-10 09:00".into(),
                updated_at: "2026-03-10 10:00".into(),
            },
            messages: vec![HistoryMessage {
                role: "user".into(),
                content: "Hello world".into(),
                author: Some("tester".into()),
                created_at: "2026-03-10 10:00".into(),
            }],
        }
    }

    fn sample_queue_message(id: i32, status: MessageStatus) -> QueueMessage {
        QueueMessage {
            id,
            session_key: "discord:test:ops".into(),
            team_run_id: "run-1".into(),
            sender: "agent-a".into(),
            recipient: "agent-b".into(),
            content: "do the thing".into(),
            msg_type: MessageType::Task,
            status,
            retry_count: 1,
            max_retries: 3,
            created_at: "2026-03-10 10:00".into(),
            processed_at: None,
            error: None,
        }
    }

    fn sample_agent_message(
        to_agent: Option<&str>,
        channel: Option<&str>,
        status: AgentMessageStatus,
    ) -> AgentMessage {
        AgentMessage {
            id: 1,
            session_key: "discord:test:ops".into(),
            from_agent: "sender-agent".into(),
            to_agent: to_agent.map(str::to_string),
            channel: channel.map(str::to_string),
            payload: "payload content".into(),
            status,
            created_at: "2026-03-10 10:00".into(),
            delivered_at: None,
        }
    }

    fn minimal_profile(title: &str) -> AgentProfile {
        AgentProfile {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            instructions: None,
            prompt: None,
            extensions: vec![],
            skills: vec![],
            settings: None,
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
        }
    }

    // --- format_duration ---

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(45), "45s");
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(90), "1m 30s");
    }

    #[test]
    fn format_duration_hours_and_minutes() {
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(0), "0s");
    }

    // --- ratio_percent ---

    #[test]
    fn ratio_percent_basic() {
        assert_eq!(ratio_percent(1, 4), 25);
    }

    #[test]
    fn ratio_percent_zero_denominator_returns_zero() {
        assert_eq!(ratio_percent(5, 0), 0);
    }

    #[test]
    fn ratio_percent_full() {
        assert_eq!(ratio_percent(3, 3), 100);
    }

    // --- tone helpers ---

    #[test]
    fn message_tone_all_variants() {
        assert_eq!(message_tone(&AgentMessageStatus::Pending), "amber");
        assert_eq!(message_tone(&AgentMessageStatus::Delivered), "cyan");
        assert_eq!(message_tone(&AgentMessageStatus::Acknowledged), "sage");
    }

    #[test]
    fn platform_tone_known_and_unknown() {
        assert_eq!(platform_tone("discord"), "cyan");
        assert_eq!(platform_tone("telegram"), "sage");
        assert_eq!(platform_tone("slack"), "amber");
        assert_eq!(platform_tone("matrix"), "neutral");
    }

    #[test]
    fn run_tone_all_variants() {
        assert_eq!(run_tone(&RunStatus::Running), "cyan");
        assert_eq!(run_tone(&RunStatus::Completed), "sage");
        assert_eq!(run_tone(&RunStatus::Failed), "rose");
        assert_eq!(run_tone(&RunStatus::Suspended), "amber");
    }

    #[test]
    fn work_tone_all_variants() {
        assert_eq!(work_tone(&WorkStatus::Pending), "amber");
        assert_eq!(work_tone(&WorkStatus::InProgress), "cyan");
        assert_eq!(work_tone(&WorkStatus::Completed), "sage");
        assert_eq!(work_tone(&WorkStatus::Failed), "rose");
        assert_eq!(work_tone(&WorkStatus::Cancelled), "neutral");
    }

    #[test]
    fn queue_tone_all_variants() {
        assert_eq!(queue_tone(&MessageStatus::Pending), "amber");
        assert_eq!(queue_tone(&MessageStatus::Processing), "cyan");
        assert_eq!(queue_tone(&MessageStatus::Completed), "sage");
        assert_eq!(queue_tone(&MessageStatus::Failed), "rose");
        assert_eq!(queue_tone(&MessageStatus::Dead), "rose");
    }

    // --- preview ---

    #[test]
    fn preview_short_text_unchanged() {
        assert_eq!(preview("hello", 10), "hello");
    }

    #[test]
    fn preview_exact_length_unchanged() {
        assert_eq!(preview("hello", 5), "hello");
    }

    #[test]
    fn preview_truncates_with_ellipsis() {
        let result = preview("hello world", 5);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn preview_empty_string() {
        assert_eq!(preview("", 10), "");
    }

    // --- progress_label ---

    #[test]
    fn progress_label_formats_steps() {
        let run = sample_run(
            "r1",
            RunStatus::Running,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        );
        assert_eq!(progress_label(&run), "1/3 steps");
    }

    // --- queue_total ---

    #[test]
    fn queue_total_sums_all_fields() {
        let stats = QueueStats {
            pending: 2,
            processing: 3,
            completed: 10,
            failed: 1,
            dead: 1,
        };
        assert_eq!(queue_total(&stats), 17);
    }

    #[test]
    fn queue_total_all_zero() {
        let stats = QueueStats {
            pending: 0,
            processing: 0,
            completed: 0,
            failed: 0,
            dead: 0,
        };
        assert_eq!(queue_total(&stats), 0);
    }

    // --- build_status_segments ---

    #[test]
    fn build_status_segments_proportional_widths() {
        let segs = build_status_segments(vec![("A", 1, "cyan"), ("B", 3, "sage")]);
        assert_eq!(segs.len(), 2);
        assert!(segs[1].width > segs[0].width);
    }

    #[test]
    fn build_status_segments_zero_total_equal_widths() {
        let segs = build_status_segments(vec![("A", 0, "cyan"), ("B", 0, "sage")]);
        // All zero segments are kept when total == 0
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].width, segs[1].width);
    }

    #[test]
    fn build_status_segments_filters_zero_entries_when_total_nonzero() {
        let segs = build_status_segments(vec![("A", 0, "cyan"), ("B", 5, "sage")]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].label, "B");
    }

    // --- duration_stats ---

    #[test]
    fn duration_stats_empty_has_no_average() {
        let stats = duration_stats(&[]);
        assert!(stats.average_label.is_none());
    }

    #[test]
    fn duration_stats_computes_average_and_max() {
        let runs = vec![
            sample_run(
                "r1",
                RunStatus::Completed,
                "2026-03-10 10:00:00",
                "2026-03-10 10:02:00",
            ),
            sample_run(
                "r2",
                RunStatus::Completed,
                "2026-03-10 10:00:00",
                "2026-03-10 10:04:00",
            ),
        ];
        let stats = duration_stats(&runs);
        assert!(stats.average_label.is_some());
        assert_eq!(stats.average_label.unwrap(), "3m 0s");
        assert!(stats.note.contains("longest"));
    }

    // --- parse_timestamp ---

    #[test]
    fn build_duration_bars_scales_with_run_length() {
        let runs = vec![
            sample_run(
                "run-a",
                RunStatus::Completed,
                "2026-03-10 10:00:00",
                "2026-03-10 10:30:00",
            ),
            sample_run(
                "run-b",
                RunStatus::Completed,
                "2026-03-10 10:00:00",
                "2026-03-10 10:05:00",
            ),
        ];

        let bars = build_duration_bars(&runs);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].value, "30m 0s");
        assert!(bars[0].height > bars[1].height);
    }

    #[test]
    fn build_duration_bars_empty_returns_empty() {
        assert!(build_duration_bars(&[]).is_empty());
    }

    #[test]
    fn parse_timestamp_accepts_minute_and_second_precision() {
        assert!(parse_timestamp("2026-03-10 10:15:42").is_some());
        assert!(parse_timestamp("2026-03-10 10:15").is_some());
        assert!(parse_timestamp("2026/03/10 10:15").is_none());
    }

    // --- activity_meta ---

    #[test]
    fn activity_meta_directed_to_agent() {
        let msg = sample_agent_message(Some("reviewer"), None, AgentMessageStatus::Pending);
        let meta = activity_meta(&msg);
        assert!(meta.contains("Directed to reviewer"));
        assert!(meta.contains("pending"));
    }

    #[test]
    fn activity_meta_published_to_channel() {
        let msg = sample_agent_message(None, Some("ops"), AgentMessageStatus::Delivered);
        let meta = activity_meta(&msg);
        assert!(meta.contains("Published to #ops"));
        assert!(meta.contains("delivered"));
    }

    #[test]
    fn activity_meta_plain_broadcast() {
        let msg = sample_agent_message(None, None, AgentMessageStatus::Acknowledged);
        let meta = activity_meta(&msg);
        assert!(meta.contains("discord:test:ops"));
        assert!(meta.contains("acknowledged"));
    }

    // --- choose_selected_name ---

    #[test]
    fn choose_selected_name_returns_match() {
        let options = vec!["alpha".into(), "beta".into()];
        assert_eq!(choose_selected_name(options, Some("beta".into())), "beta");
    }

    #[test]
    fn choose_selected_name_falls_back_to_first() {
        let options = vec!["alpha".into(), "beta".into()];
        assert_eq!(choose_selected_name(options, Some("gamma".into())), "alpha");
    }

    #[test]
    fn choose_selected_name_none_falls_back_to_first() {
        let options = vec!["alpha".into(), "beta".into()];
        assert_eq!(choose_selected_name(options, None), "alpha");
    }

    // --- choose_selected_run ---

    #[test]
    fn choose_selected_run_returns_match() {
        let runs = vec![
            sample_run(
                "run-1",
                RunStatus::Running,
                "2026-03-10 10:00",
                "2026-03-10 10:05",
            ),
            sample_run(
                "run-2",
                RunStatus::Completed,
                "2026-03-10 09:00",
                "2026-03-10 09:10",
            ),
        ];
        assert_eq!(choose_selected_run(&runs, Some("run-2".into())), "run-2");
    }

    #[test]
    fn choose_selected_run_falls_back_to_first() {
        let runs = vec![sample_run(
            "run-1",
            RunStatus::Running,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        )];
        assert_eq!(choose_selected_run(&runs, Some("unknown".into())), "run-1");
    }

    // --- choose_selected_session ---

    #[test]
    fn choose_selected_session_returns_match() {
        let sessions = vec![
            sample_session("discord:ns:chan-a", Some("team-1")),
            sample_session("telegram:direct:user-1", None),
        ];
        let key = choose_selected_session(&sessions, Some("telegram:direct:user-1".into()));
        assert_eq!(key, "telegram:direct:user-1");
    }

    #[test]
    fn choose_selected_session_falls_back_to_first() {
        let sessions = vec![sample_session("discord:ns:chan-a", Some("team-1"))];
        let key = choose_selected_session(&sessions, Some("does-not-exist".into()));
        assert_eq!(key, "discord:ns:chan-a");
    }

    // --- build_session_list_items ---

    #[test]
    fn build_session_list_items_sets_active_flag() {
        let sessions = vec![
            sample_session("discord:ns:chan-a", Some("team-1")),
            sample_session("telegram:direct:user-1", None),
        ];
        let items =
            build_session_list_items(&sessions, Some("telegram:direct:user-1".into()), "Live");
        assert!(!items[0].active);
        assert!(items[1].active);
    }

    #[test]
    fn build_session_list_items_discord_badge_tone() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Mock");
        assert_eq!(items[0].badge, "DISCORD");
        assert_eq!(items[0].badge_tone, "cyan");
    }

    #[test]
    fn build_session_list_items_with_active_team_subtitle() {
        let sessions = vec![sample_session("discord:ns:chan-a", Some("feature-dev"))];
        let items = build_session_list_items(&sessions, None, "Live runtime");
        assert!(items[0].subtitle.contains("feature-dev"));
        assert!(items[0].subtitle.contains("Live runtime"));
    }

    #[test]
    fn build_session_list_items_no_active_team_subtitle() {
        let sessions = vec![sample_session("discord:ns:chan-a", None)];
        let items = build_session_list_items(&sessions, None, "Live runtime");
        assert!(items[0].subtitle.contains("No active team"));
    }

    // --- build_session_detail ---

    #[test]
    fn build_session_detail_with_namespace() {
        // Format: platform:ns:<namespace>:<channel_id>
        let session = sample_session("discord:ns:studio-a:ops-bridge", Some("team-1"));
        let detail = build_session_detail(&session, "Mock preview");
        assert!(detail.title.contains("ops-bridge"));
        assert!(detail.subtitle.contains("discord"));
        assert!(detail.subtitle.contains("studio-a"));
    }

    #[test]
    fn build_session_detail_without_namespace() {
        let session = sample_session("telegram:direct:user-1", None);
        let detail = build_session_detail(&session, "Live");
        assert!(detail.subtitle.contains("telegram"));
        assert!(detail.subtitle.contains("direct"));
    }

    #[test]
    fn build_session_detail_message_bubble_role_alignment() {
        let mut session = sample_session("discord:ns:chan-a", None);
        session.messages.push(HistoryMessage {
            role: "assistant".into(),
            content: "I can help".into(),
            author: Some("goose".into()),
            created_at: "2026-03-10 10:01".into(),
        });
        let detail = build_session_detail(&session, "Mock");
        let user_bubble = &detail.messages[0];
        let assistant_bubble = &detail.messages[1];
        assert_eq!(user_bubble.alignment, "left");
        assert_eq!(user_bubble.tone, "plain");
        assert_eq!(assistant_bubble.alignment, "right");
        assert_eq!(assistant_bubble.tone, "accent");
    }

    #[test]
    fn build_session_detail_active_team_meta_row() {
        let session = sample_session("discord:ns:chan-a", Some("feature-dev"));
        let detail = build_session_detail(&session, "Mock");
        let active_team_row = detail
            .meta
            .iter()
            .find(|row| row.label == "Active team")
            .unwrap();
        assert_eq!(active_team_row.value, "feature-dev");
    }

    #[test]
    fn build_session_detail_no_active_team_shows_none() {
        let session = sample_session("discord:ns:chan-a", None);
        let detail = build_session_detail(&session, "Mock");
        let active_team_row = detail
            .meta
            .iter()
            .find(|row| row.label == "Active team")
            .unwrap();
        assert_eq!(active_team_row.value, "None");
    }

    // --- build_run_list_items ---

    #[test]
    fn build_run_list_items_active_flag() {
        let runs = vec![
            sample_run(
                "run-1",
                RunStatus::Running,
                "2026-03-10 10:00",
                "2026-03-10 10:05",
            ),
            sample_run(
                "run-2",
                RunStatus::Completed,
                "2026-03-10 09:00",
                "2026-03-10 09:10",
            ),
        ];
        let items = build_run_list_items(&runs, Some("run-2".into()), "Live");
        assert!(!items[0].active);
        assert!(items[1].active);
    }

    #[test]
    fn build_run_list_items_badge_is_status_uppercased() {
        let runs = vec![sample_run(
            "run-1",
            RunStatus::Suspended,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        )];
        let items = build_run_list_items(&runs, None, "Mock");
        assert_eq!(items[0].badge, "SUSPENDED");
    }

    #[test]
    fn build_run_list_items_page_urls() {
        let runs = vec![sample_run(
            "run-1",
            RunStatus::Running,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        )];
        let items = build_run_list_items(&runs, None, "Live");
        assert!(items[0].page_url.contains("/runs?run="));
        assert!(items[0].queue_page_url.contains("/queue?run="));
    }

    // --- build_run_detail ---

    #[test]
    fn build_run_detail_title_and_subtitle() {
        let run = sample_run(
            "run-42",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:10:00",
        );
        let detail = build_run_detail(&run, &[], &[], "Live runtime");
        assert_eq!(detail.title, "Run run-42");
        assert!(detail.subtitle.contains("team-run-42"));
        assert!(detail.subtitle.contains("chain"));
    }

    #[test]
    fn build_run_detail_work_item_indent_class() {
        let run = sample_run(
            "r1",
            RunStatus::Running,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        );
        let work_items = vec![
            WorkItem {
                id: 1,
                session_key: "discord:test:ops".into(),
                team_run_id: "r1".into(),
                parent_id: None,
                title: "Root task".into(),
                description: None,
                status: WorkStatus::Completed,
                assigned_to: Some("agent-a".into()),
                workflow_step: Some(0),
                input: None,
                output: None,
                error: None,
                created_at: "2026-03-10 10:00".into(),
                updated_at: "2026-03-10 10:05".into(),
            },
            WorkItem {
                id: 2,
                session_key: "discord:test:ops".into(),
                team_run_id: "r1".into(),
                parent_id: Some(1),
                title: "Child task".into(),
                description: None,
                status: WorkStatus::InProgress,
                assigned_to: None,
                workflow_step: Some(1),
                input: None,
                output: None,
                error: None,
                created_at: "2026-03-10 10:02".into(),
                updated_at: "2026-03-10 10:04".into(),
            },
        ];
        let detail = build_run_detail(&run, &work_items, &[], "Live");
        assert_eq!(detail.work_items[0].indent_class, "is-root");
        assert_eq!(detail.work_items[1].indent_class, "is-child");
    }

    #[test]
    fn build_run_detail_no_result_shows_placeholder() {
        let run = sample_run(
            "r1",
            RunStatus::Running,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        );
        let detail = build_run_detail(&run, &[], &[], "Live");
        assert!(detail.result.contains("No final result"));
    }

    // --- build_queue_row ---

    #[test]
    fn build_queue_row_retry_text() {
        let msg = sample_queue_message(1, MessageStatus::Pending);
        let view = build_queue_row(&msg);
        assert_eq!(view.retry_text, "1/3");
    }

    #[test]
    fn build_queue_row_status_tone_and_label() {
        let msg = sample_queue_message(1, MessageStatus::Completed);
        let view = build_queue_row(&msg);
        assert_eq!(view.status_tone, "sage");
        assert_eq!(view.status_label, "completed");
    }

    #[test]
    fn build_queue_row_error_defaults_to_empty() {
        let msg = sample_queue_message(1, MessageStatus::Pending);
        let view = build_queue_row(&msg);
        assert_eq!(view.error, "");
    }

    #[test]
    fn build_queue_row_error_populated_when_set() {
        let mut msg = sample_queue_message(1, MessageStatus::Failed);
        msg.error = Some("timeout".into());
        let view = build_queue_row(&msg);
        assert_eq!(view.error, "timeout");
    }

    // --- build_queue_detail ---

    #[test]
    fn build_queue_detail_title_and_status_cards() {
        let run = sample_run(
            "run-q",
            RunStatus::Running,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        );
        let stats = QueueStats {
            pending: 2,
            processing: 1,
            completed: 5,
            failed: 0,
            dead: 1,
        };
        let detail = build_queue_detail(&run, &[], &[], &stats, "Live");
        assert_eq!(detail.title, "Queue run-q");
        assert_eq!(detail.status_cards.len(), 4);
        let pending_card = &detail.status_cards[0];
        assert_eq!(pending_card.value, "2");
    }

    #[test]
    fn build_queue_detail_dead_letter_separation() {
        let run = sample_run(
            "run-q",
            RunStatus::Running,
            "2026-03-10 10:00",
            "2026-03-10 10:05",
        );
        let stats = QueueStats {
            pending: 0,
            processing: 0,
            completed: 1,
            failed: 0,
            dead: 1,
        };
        let live_msg = sample_queue_message(1, MessageStatus::Completed);
        let dead_msg = sample_queue_message(2, MessageStatus::Dead);
        let detail = build_queue_detail(&run, &[live_msg], &[dead_msg], &stats, "Live");
        assert_eq!(detail.messages.len(), 1);
        assert_eq!(detail.dead_letters.len(), 1);
    }

    // --- capability_line ---

    #[test]
    fn capability_line_with_settings() {
        let mut profile = minimal_profile("my-agent");
        profile.settings = Some(ProfileSettings {
            goose_provider: Some("anthropic".into()),
            goose_model: Some("claude-sonnet".into()),
            ..Default::default()
        });
        assert_eq!(capability_line(&profile), "anthropic / claude-sonnet");
    }

    #[test]
    fn capability_line_without_settings() {
        let profile = minimal_profile("bare-agent");
        assert_eq!(capability_line(&profile), "provider unset / model unset");
    }

    // --- profile_settings ---

    #[test]
    fn profile_settings_with_full_settings() {
        let mut profile = minimal_profile("configured-agent");
        profile.settings = Some(ProfileSettings {
            goose_provider: Some("openai".into()),
            goose_model: Some("gpt-4".into()),
            temperature: Some(0.7),
            max_turns: Some(10),
            max_retries: Some(3),
            ..Default::default()
        });
        let rows = profile_settings(&profile);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Provider"));
        assert!(labels.contains(&"Model"));
        assert!(labels.contains(&"Temperature"));
        assert!(labels.contains(&"Max turns"));
        assert!(labels.contains(&"Retries"));
    }

    #[test]
    fn profile_settings_empty_shows_placeholder() {
        let profile = minimal_profile("bare-agent");
        let rows = profile_settings(&profile);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Settings");
        assert!(rows[0].value.contains("No explicit"));
    }

    // --- WorkflowName trait ---

    #[test]
    fn workflow_name_all_patterns() {
        let make = |workflow| TeamDefinition {
            version: "1.0.0".into(),
            title: "test-team".into(),
            description: None,
            workflow,
            agents: vec![],
            router: None,
            fan_out: None,
        };
        assert_eq!(make(OrchestrationPattern::Chain).workflow_name(), "Chain");
        assert_eq!(
            make(OrchestrationPattern::FanOut).workflow_name(),
            "Fan-out"
        );
        assert_eq!(make(OrchestrationPattern::Router).workflow_name(), "Router");
    }

    // --- build_agent_detail ---

    #[test]
    fn build_agent_detail_extension_rows() {
        let entry = ProfileCatalogEntry {
            profile: AgentProfile {
                version: "1.0.0".into(),
                title: "test-agent".into(),
                description: Some("A test agent".into()),
                instructions: Some("Do stuff".into()),
                prompt: None,
                extensions: vec![ExtensionRef {
                    name: "my-tool".into(),
                    ext_type: "builtin".into(),
                    cmd: Some("npx my-tool".into()),
                    args: vec![],
                    uri: None,
                    timeout: None,
                    envs: std::collections::HashMap::new(),
                    env_keys: vec![],
                    code: None,
                    dependencies: None,
                }],
                skills: vec!["skill-a".into()],
                settings: None,
                activities: None,
                response: None,
                sub_recipes: None,
                parameters: None,
            },
            source_label: "test source".into(),
            is_live: true,
        };
        let detail = build_agent_detail(&entry).unwrap();
        assert_eq!(detail.title, "test-agent");
        assert_eq!(detail.extensions.len(), 1);
        assert_eq!(detail.extensions[0].name, "my-tool");
        assert_eq!(detail.extensions[0].summary, "npx my-tool");
        assert_eq!(detail.skills.len(), 1);
    }

    #[test]
    fn build_agent_detail_instructions_preview_truncated() {
        let long_instructions = "A".repeat(500);
        let mut profile = minimal_profile("verbose-agent");
        profile.instructions = Some(long_instructions);
        let entry = ProfileCatalogEntry {
            profile,
            source_label: "test".into(),
            is_live: false,
        };
        let detail = build_agent_detail(&entry).unwrap();
        assert!(detail.instructions_preview.ends_with("..."));
    }

    #[test]
    fn display_status_label_humanizes_snake_case() {
        assert_eq!(display_status_label("in_progress"), "In Progress");
        assert_eq!(display_status_label("running"), "Running");
    }

    // --- View struct construction and Clone ---

    #[test]
    fn metric_card_fields_stored_correctly() {
        let card = MetricCard {
            label: "Sessions".into(),
            value: "42".into(),
            note: "last 24h".into(),
            tone: "success",
        };
        assert_eq!(card.label, "Sessions");
        assert_eq!(card.value, "42");
        assert_eq!(card.note, "last 24h");
        assert_eq!(card.tone, "success");
    }

    #[test]
    fn metric_card_clone_is_independent() {
        let card = MetricCard {
            label: "A".into(),
            value: "1".into(),
            note: "note".into(),
            tone: "neutral",
        };
        let mut cloned = card.clone();
        cloned.label = "B".into();
        assert_eq!(card.label, "A");
        assert_eq!(cloned.label, "B");
    }

    #[test]
    fn metric_card_empty_strings_accepted() {
        let card = MetricCard {
            label: String::new(),
            value: String::new(),
            note: String::new(),
            tone: "",
        };
        assert!(card.label.is_empty());
    }

    #[test]
    fn alert_card_fields_stored_correctly() {
        let card = AlertCard {
            eyebrow: "WARNING".into(),
            title: "Channel disconnected".into(),
            description: "Slack gateway lost connection.".into(),
            tone: "danger",
        };
        assert_eq!(card.eyebrow, "WARNING");
        assert_eq!(card.title, "Channel disconnected");
        assert_eq!(card.tone, "danger");
    }

    #[test]
    fn alert_card_clone_is_independent() {
        let card = AlertCard {
            eyebrow: "INFO".into(),
            title: "Title".into(),
            description: "Desc".into(),
            tone: "info",
        };
        let mut cloned = card.clone();
        cloned.title = "Other".into();
        assert_eq!(card.title, "Title");
        assert_eq!(cloned.title, "Other");
    }

    #[test]
    fn meta_row_fields_stored_correctly() {
        let row = MetaRow {
            label: "Status".into(),
            value: "Running".into(),
        };
        assert_eq!(row.label, "Status");
        assert_eq!(row.value, "Running");
    }

    #[test]
    fn meta_row_empty_value_accepted() {
        let row = MetaRow {
            label: "Result".into(),
            value: String::new(),
        };
        assert!(row.value.is_empty());
    }

    #[test]
    fn session_list_item_active_flag_stored() {
        let item = SessionListItem {
            title: "Session A".into(),
            subtitle: "discord".into(),
            preview: "Hello".into(),
            updated_at: "2026-03-10".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: "/sessions?key=abc".into(),
            active: true,
        };
        assert!(item.active);
        assert_eq!(item.badge_tone, "cyan");
    }

    #[test]
    fn session_list_item_inactive_by_default() {
        let item = SessionListItem {
            title: "Session B".into(),
            subtitle: "telegram".into(),
            preview: String::new(),
            updated_at: "2026-03-10".into(),
            badge: "TELEGRAM".into(),
            badge_tone: "sage",
            page_url: "/sessions?key=def".into(),
            active: false,
        };
        assert!(!item.active);
    }

    #[test]
    fn run_list_item_urls_stored_correctly() {
        let item = RunListItem {
            title: "Run 1".into(),
            subtitle: "team / chain".into(),
            updated_at: "2026-03-10".into(),
            progress_label: "2 / 4".into(),
            badge: "RUNNING".into(),
            badge_tone: "cyan",
            page_url: "/runs?run=r1".into(),
            queue_page_url: "/queue?run=r1".into(),
            active: true,
        };
        assert!(item.page_url.contains("/runs?run="));
        assert!(item.queue_page_url.contains("/queue?run="));
        assert!(item.active);
    }

    #[test]
    fn run_list_item_clone_preserves_fields() {
        let item = RunListItem {
            title: "T".into(),
            subtitle: "S".into(),
            updated_at: "U".into(),
            progress_label: "P".into(),
            badge: "B".into(),
            badge_tone: "neutral",
            page_url: "/runs?run=x".into(),
            queue_page_url: "/queue?run=x".into(),
            active: false,
        };
        let cloned = item.clone();
        assert_eq!(cloned.title, item.title);
        assert_eq!(cloned.badge_tone, item.badge_tone);
    }

    #[test]
    fn work_item_view_root_indent_class() {
        let item = WorkItemView {
            title: "Root task".into(),
            detail: String::new(),
            status_label: "Pending".into(),
            status_tone: "neutral",
            step_label: "Step 0".into(),
            indent_class: "is-root",
        };
        assert_eq!(item.indent_class, "is-root");
    }

    #[test]
    fn work_item_view_child_indent_class() {
        let item = WorkItemView {
            title: "Child task".into(),
            detail: "assigned to agent".into(),
            status_label: "In Progress".into(),
            status_tone: "cyan",
            step_label: "Step 1".into(),
            indent_class: "is-child",
        };
        assert_eq!(item.indent_class, "is-child");
        assert_eq!(item.status_tone, "cyan");
    }

    #[test]
    fn broadcast_view_fields_stored_correctly() {
        let bv = BroadcastView {
            sender: "planner".into(),
            created_at: "2026-03-10 10:00".into(),
            content: "Go ahead.".into(),
        };
        assert_eq!(bv.sender, "planner");
        assert_eq!(bv.content, "Go ahead.");
    }

    #[test]
    fn broadcast_view_empty_content_accepted() {
        let bv = BroadcastView {
            sender: String::new(),
            created_at: String::new(),
            content: String::new(),
        };
        assert!(bv.content.is_empty());
    }

    #[test]
    fn run_detail_view_empty_collections() {
        let view = RunDetailView {
            title: "Run X".into(),
            subtitle: "team / chain".into(),
            source_label: "Discord".into(),
            meta: vec![],
            work_items: vec![],
            broadcasts: vec![],
            input: "some input".into(),
            result: "No final result yet.".into(),
            empty_hint: "No work items.".into(),
        };
        assert!(view.work_items.is_empty());
        assert!(view.broadcasts.is_empty());
    }

    #[test]
    fn run_detail_view_result_field_stored() {
        let view = RunDetailView {
            title: "Run Y".into(),
            subtitle: String::new(),
            source_label: String::new(),
            meta: vec![],
            work_items: vec![],
            broadcasts: vec![],
            input: String::new(),
            result: "All done.".into(),
            empty_hint: String::new(),
        };
        assert_eq!(view.result, "All done.");
    }

    #[test]
    fn queue_message_view_fields_stored_correctly() {
        let msg = QueueMessageView {
            sender: "planner".into(),
            recipient: "worker".into(),
            kind: "Task".into(),
            status_label: "Completed".into(),
            status_tone: "sage",
            created_at: "2026-03-10 10:00".into(),
            retry_text: String::new(),
            content: "do this".into(),
            error: String::new(),
        };
        assert_eq!(msg.sender, "planner");
        assert_eq!(msg.status_tone, "sage");
        assert!(msg.error.is_empty());
    }

    #[test]
    fn queue_message_view_error_field_stored() {
        let msg = QueueMessageView {
            sender: "planner".into(),
            recipient: "worker".into(),
            kind: "Task".into(),
            status_label: "Failed".into(),
            status_tone: "rose",
            created_at: "2026-03-10 10:00".into(),
            retry_text: "Retry 1/3".into(),
            content: "do this".into(),
            error: "timeout".into(),
        };
        assert_eq!(msg.error, "timeout");
        assert_eq!(msg.retry_text, "Retry 1/3");
    }

    #[test]
    fn queue_detail_view_empty_messages_and_dead_letters() {
        let view = QueueDetailView {
            title: "Queue: Run 1".into(),
            subtitle: "team / chain".into(),
            source_label: "Live".into(),
            status_cards: vec![],
            messages: vec![],
            dead_letters: vec![],
            empty_hint: "No messages.".into(),
        };
        assert!(view.messages.is_empty());
        assert!(view.dead_letters.is_empty());
    }

    #[test]
    fn queue_detail_view_with_status_cards() {
        let view = QueueDetailView {
            title: "Queue: Run 1".into(),
            subtitle: String::new(),
            source_label: String::new(),
            status_cards: vec![
                MetricCard {
                    label: "Total".into(),
                    value: "10".into(),
                    note: String::new(),
                    tone: "neutral",
                },
                MetricCard {
                    label: "Dead".into(),
                    value: "2".into(),
                    note: String::new(),
                    tone: "rose",
                },
            ],
            messages: vec![],
            dead_letters: vec![],
            empty_hint: String::new(),
        };
        assert_eq!(view.status_cards.len(), 2);
        assert_eq!(view.status_cards[1].tone, "rose");
    }

    #[test]
    fn agent_list_item_active_flag_stored() {
        let item = AgentListItem {
            title: "Claude Coder".into(),
            subtitle: "claude_local".into(),
            capability: "coding".into(),
            source_label: "Local".into(),
            page_url: "/agents?agent=coder".into(),
            active: true,
        };
        assert_eq!(item.title, "Claude Coder");
        assert!(item.active);
    }

    #[test]
    fn agent_detail_view_empty_collections() {
        let view = AgentDetailView {
            title: "Agent".into(),
            subtitle: "claude_local".into(),
            source_label: "Local".into(),
            instructions_preview: String::new(),
            settings: vec![],
            activities: vec![],
            skills: vec![],
            extensions: vec![],
            yaml: String::new(),
        };
        assert!(view.settings.is_empty());
        assert!(view.skills.is_empty());
        assert!(view.extensions.is_empty());
    }

    #[test]
    fn notice_fields_stored_correctly() {
        let notice = Notice {
            text: "Saved successfully.".into(),
            tone: "success",
        };
        assert_eq!(notice.text, "Saved successfully.");
        assert_eq!(notice.tone, "success");
    }

    #[test]
    fn notice_clone_is_independent() {
        let notice = Notice {
            text: "Original".into(),
            tone: "neutral",
        };
        let mut cloned = notice.clone();
        cloned.text = "Changed".into();
        assert_eq!(notice.text, "Original");
        assert_eq!(cloned.text, "Changed");
    }

    #[test]
    fn team_editor_view_no_notice() {
        let view = TeamEditorView {
            title: "Edit Team".into(),
            subtitle: "feature-dev".into(),
            source_label: "Live".into(),
            workflow_label: "chain".into(),
            members_text: "3 agents".into(),
            original_name: "feature-dev".into(),
            yaml: "name: feature-dev".into(),
            notice: None,
        };
        assert!(view.notice.is_none());
    }

    #[test]
    fn team_editor_view_with_notice() {
        let view = TeamEditorView {
            title: "Edit Team".into(),
            subtitle: "feature-dev".into(),
            source_label: "Live".into(),
            workflow_label: "chain".into(),
            members_text: "3 agents".into(),
            original_name: "feature-dev".into(),
            yaml: "name: feature-dev".into(),
            notice: Some(Notice {
                text: "Saved.".into(),
                tone: "success",
            }),
        };
        assert!(view.notice.is_some());
        assert_eq!(view.notice.unwrap().tone, "success");
    }

    #[test]
    fn sessions_page_view_mock_mode() {
        let view = SessionsPageView {
            mode_label: "Mock preview".into(),
            mode_tone: "neutral",
            sessions: vec![],
            selected: SessionDetailView {
                title: String::new(),
                subtitle: String::new(),
                source_label: String::new(),
                meta: vec![],
                messages: vec![],
                empty_hint: "No data.".into(),
            },
        };
        assert_eq!(view.mode_label, "Mock preview");
        assert_eq!(view.mode_tone, "neutral");
        assert!(view.sessions.is_empty());
    }

    #[test]
    fn runs_page_view_live_mode() {
        let view = RunsPageView {
            mode_label: "Live runtime".into(),
            mode_tone: "success",
            runs: vec![],
            selected: RunDetailView {
                title: String::new(),
                subtitle: String::new(),
                source_label: String::new(),
                meta: vec![],
                work_items: vec![],
                broadcasts: vec![],
                input: String::new(),
                result: String::new(),
                empty_hint: String::new(),
            },
        };
        assert_eq!(view.mode_tone, "success");
        assert!(view.runs.is_empty());
    }

    #[test]
    fn teams_page_view_empty_teams() {
        let view = TeamsPageView {
            mode_label: "Mock preview".into(),
            mode_tone: "neutral",
            teams: vec![],
            selected: TeamEditorView {
                title: String::new(),
                subtitle: String::new(),
                source_label: String::new(),
                workflow_label: String::new(),
                members_text: String::new(),
                original_name: String::new(),
                yaml: String::new(),
                notice: None,
            },
        };
        assert_eq!(view.mode_label, "Mock preview");
        assert!(view.teams.is_empty());
    }
}
