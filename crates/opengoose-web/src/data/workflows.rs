use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, Schedule, ScheduleStore, Trigger, TriggerStore,
};
use opengoose_teams::{TeamDefinition, TeamStore, all_defaults as default_teams};
use urlencoding::encode;

use crate::data::utils::{choose_selected_name, preview, progress_label, run_tone};
use crate::data::views::{
    MetaRow, WorkflowAutomationView, WorkflowDetailView, WorkflowListItem, WorkflowRunView,
    WorkflowStepView, WorkflowsPageView,
};

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
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;

    fn minimal_team(title: &str) -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![TeamAgent {
                profile: "agent-a".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
        }
    }

    #[test]
    fn display_status_label_single_word() {
        assert_eq!(display_status_label("running"), "Running");
    }

    #[test]
    fn display_status_label_underscored() {
        assert_eq!(display_status_label("in_progress"), "In Progress");
    }

    #[test]
    fn display_status_label_empty() {
        assert_eq!(display_status_label(""), "");
    }

    #[test]
    fn enabled_total_label_zero() {
        assert_eq!(enabled_total_label(0, 0), "0 configured");
    }

    #[test]
    fn enabled_total_label_some() {
        assert_eq!(enabled_total_label(2, 3), "2/3 enabled");
    }

    #[test]
    fn step_prefix_chain() {
        assert_eq!(step_prefix(&OrchestrationPattern::Chain, 0), "Step 1");
    }

    #[test]
    fn step_prefix_fan_out() {
        assert_eq!(step_prefix(&OrchestrationPattern::FanOut, 2), "Branch 3");
    }

    #[test]
    fn step_prefix_router() {
        assert_eq!(step_prefix(&OrchestrationPattern::Router, 0), "Route 1");
    }

    #[test]
    fn step_badge_labels() {
        assert_eq!(step_badge(&OrchestrationPattern::Chain), "Sequential");
        assert_eq!(step_badge(&OrchestrationPattern::FanOut), "Parallel");
        assert_eq!(step_badge(&OrchestrationPattern::Router), "Candidate");
    }

    #[test]
    fn step_badge_tone_values() {
        assert_eq!(step_badge_tone(&OrchestrationPattern::Chain), "cyan");
        assert_eq!(step_badge_tone(&OrchestrationPattern::FanOut), "amber");
        assert_eq!(step_badge_tone(&OrchestrationPattern::Router), "sage");
    }

    #[test]
    fn workflow_status_manual_when_no_runs_or_automations() {
        let entry = WorkflowCatalogEntry {
            name: "test".into(),
            team: minimal_team("test"),
            source_label: "Bundled default".into(),
            schedules: vec![],
            triggers: vec![],
            recent_runs: vec![],
        };
        let (label, tone) = workflow_status(&entry);
        assert_eq!(label, "Manual");
        assert_eq!(tone, "neutral");
    }

    #[test]
    fn automation_summary_manual_only() {
        let entry = WorkflowCatalogEntry {
            name: "test".into(),
            team: minimal_team("test"),
            source_label: "Bundled default".into(),
            schedules: vec![],
            triggers: vec![],
            recent_runs: vec![],
        };
        assert_eq!(automation_summary(&entry), "Manual only");
    }

    #[test]
    fn team_agent_summary_joins_profiles() {
        let mut team = minimal_team("test");
        team.agents.push(TeamAgent {
            profile: "agent-b".into(),
            role: None,
        });
        assert_eq!(team_agent_summary(&team), "agent-a · agent-b");
    }
}
