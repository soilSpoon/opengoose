use anyhow::{Context, Result};
use urlencoding::encode;

use super::catalog::WorkflowCatalogEntry;
use super::summary::{
    WorkflowName, automation_summary, display_status_label, enabled_total_label, step_badge,
    step_badge_tone, step_prefix, team_agent_summary, workflow_status,
};
use crate::data::utils::{choose_selected_name, preview, progress_label, run_tone};
use crate::data::views::{
    MetaRow, WorkflowAutomationView, WorkflowDetailView, WorkflowListItem, WorkflowRunView,
    WorkflowStepView, WorkflowsPageView,
};

pub(super) fn build_workflows_page(
    catalog: &[WorkflowCatalogEntry],
    using_preview: bool,
    selected: Option<String>,
) -> Result<WorkflowsPageView> {
    let selected_name = choose_selected_name(
        catalog.iter().map(|entry| entry.name.clone()).collect(),
        selected,
    );

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
        trigger_api_url: format!("/workflows/{}/trigger", encode(&entry.name)),
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

#[cfg(test)]
mod tests {
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;
    use crate::data::workflows::catalog::WorkflowCatalogEntry;

    fn minimal_team(title: &str) -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: title.into(),
            description: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![TeamAgent {
                profile: format!("{title}-agent"),
                role: None,
            }],
            router: None,
            fan_out: None,
            goal: None,
        }
    }

    fn workflow_entry(name: &str) -> WorkflowCatalogEntry {
        WorkflowCatalogEntry {
            name: name.into(),
            team: minimal_team(name),
            source_label: "Bundled default".into(),
            schedules: vec![],
            triggers: vec![],
            recent_runs: vec![],
        }
    }

    #[test]
    fn build_workflows_page_uses_requested_selection() {
        let catalog = vec![workflow_entry("alpha"), workflow_entry("beta")];
        let page = build_workflows_page(&catalog, false, Some("beta".into())).unwrap();

        assert_eq!(page.mode_label, "Live registry");
        assert_eq!(page.selected.title, "beta");
        assert_eq!(page.workflows.iter().filter(|item| item.active).count(), 1);
        assert!(
            page.workflows
                .iter()
                .any(|item| item.title == "beta" && item.active)
        );
    }

    #[test]
    fn build_workflows_page_falls_back_to_first_workflow_when_selection_is_unknown() {
        let catalog = vec![workflow_entry("alpha"), workflow_entry("beta")];
        let page = build_workflows_page(&catalog, true, Some("missing".into())).unwrap();

        assert_eq!(page.mode_label, "Bundled defaults");
        assert_eq!(page.selected.title, "alpha");
        assert!(page.workflows[0].active);
        assert!(!page.workflows[1].active);
    }
}
