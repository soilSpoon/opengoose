use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use chrono::{NaiveDateTime, Utc};
use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, HistoryMessage, MessageQueue,
    OrchestrationRun, OrchestrationStore, QueueMessage, QueueStats, RunStatus, SessionStore,
    SessionSummary, WorkItem, WorkItemStore, WorkStatus,
};
use opengoose_profiles::{AgentProfile, ProfileStore, all_defaults as default_profiles};
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
    pub detail_url: String,
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
    pub detail_url: String,
    pub queue_page_url: String,
    pub queue_detail_url: String,
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
    pub detail_url: String,
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
    pub detail_url: String,
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
                detail_url: format!("/agents/detail?agent={}", encode(&entry.profile.title)),
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
                detail_url: format!("/teams/editor?team={}", encode(&entry.team.title)),
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
    team: TeamDefinition,
    source_label: String,
    is_live: bool,
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
                detail_url: format!("/sessions/detail?session={encoded}"),
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
            detail_url: format!("/runs/detail?run={}", encode(&run.team_run_id)),
            queue_page_url: format!("/queue?run={}", encode(&run.team_run_id)),
            queue_detail_url: format!("/queue/detail?run={}", encode(&run.team_run_id)),
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
                team,
                source_label: format!("{}", store.dir().display()),
                is_live: true,
            })
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

#[cfg(test)]
mod tests {
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
    fn parse_timestamp_accepts_minute_and_second_precision() {
        assert!(parse_timestamp("2026-03-10 10:15:42").is_some());
        assert!(parse_timestamp("2026-03-10 10:15").is_some());
        assert!(parse_timestamp("2026/03/10 10:15").is_none());
    }
}
