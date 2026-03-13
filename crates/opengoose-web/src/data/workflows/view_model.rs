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
    use opengoose_persistence::{OrchestrationRun, RunStatus, Schedule, Trigger};
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
            communication_mode: Default::default(),
        }
    }

    fn team_agent(profile: &str, role: Option<&str>) -> TeamAgent {
        TeamAgent {
            profile: profile.into(),
            role: role.map(str::to_string),
        }
    }

    fn schedule(
        name: &str,
        cron_expression: &str,
        team_name: &str,
        enabled: bool,
        last_run_at: Option<&str>,
        next_run_at: Option<&str>,
    ) -> Schedule {
        Schedule {
            id: name.len() as i32,
            name: name.into(),
            cron_expression: cron_expression.into(),
            team_name: team_name.into(),
            input: String::new(),
            enabled,
            last_run_at: last_run_at.map(str::to_string),
            next_run_at: next_run_at.map(str::to_string),
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: "2026-03-12 00:00:00".into(),
        }
    }

    fn trigger(
        name: &str,
        trigger_type: &str,
        condition_json: &str,
        team_name: &str,
        enabled: bool,
        last_fired_at: Option<&str>,
        fire_count: i32,
    ) -> Trigger {
        Trigger {
            id: name.len() as i32,
            name: name.into(),
            trigger_type: trigger_type.into(),
            condition_json: condition_json.into(),
            team_name: team_name.into(),
            input: String::new(),
            enabled,
            last_fired_at: last_fired_at.map(str::to_string),
            fire_count,
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: "2026-03-12 00:00:00".into(),
        }
    }

    fn run(
        team_run_id: &str,
        team_name: &str,
        status: RunStatus,
        current_step: i32,
        total_steps: i32,
        result: Option<&str>,
        updated_at: &str,
    ) -> OrchestrationRun {
        OrchestrationRun {
            team_run_id: team_run_id.into(),
            session_key: format!("session-{team_run_id}"),
            team_name: team_name.into(),
            workflow: "chain".into(),
            input: String::new(),
            status,
            current_step,
            total_steps,
            result: result.map(str::to_string),
            created_at: "2026-03-12 00:00:00".into(),
            updated_at: updated_at.into(),
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

    fn meta_value<'a>(detail: &'a WorkflowDetailView, label: &str) -> &'a str {
        detail
            .meta
            .iter()
            .find(|row| row.label == label)
            .map(|row| row.value.as_str())
            .expect("meta row should exist")
    }

    #[test]
    fn build_workflows_page_uses_requested_selection() {
        let catalog = vec![workflow_entry("alpha"), workflow_entry("beta")];
        let page = build_workflows_page(&catalog, false, Some("beta".into())).unwrap();

        assert_eq!(page.mode_label, "Live registry");
        assert_eq!(page.mode_tone, "success");
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
        assert_eq!(page.mode_tone, "neutral");
        assert_eq!(page.selected.title, "alpha");
        assert!(page.workflows[0].active);
        assert!(!page.workflows[1].active);
    }

    #[test]
    fn build_workflow_list_item_formats_preview_status_and_encoded_url() {
        let name = "feature dev/router";
        let mut entry = workflow_entry(name);
        entry.team = TeamDefinition {
            version: "1.0.0".into(),
            title: "Feature Delivery".into(),
            description: None,
            workflow: OrchestrationPattern::FanOut,
            agents: vec![
                team_agent("planner", Some("Plan")),
                team_agent("builder", None),
            ],
            router: None,
            fan_out: None,
            goal: None,
            communication_mode: Default::default(),
        };
        entry.schedules = vec![
            schedule("nightly", "0 0 * * *", name, true, None, None),
            schedule("manual-hold", "0 12 * * *", name, false, None, None),
        ];
        entry.triggers = vec![trigger(
            "on-pr",
            "pull_request_merged",
            "{\"branch\":\"main\"}",
            name,
            false,
            None,
            0,
        )];
        entry.recent_runs = vec![run(
            "run-1",
            name,
            RunStatus::Completed,
            2,
            2,
            Some("Shipped"),
            "2026-03-12 10:00:00",
        )];

        let item = build_workflow_list_item(&entry, name);

        assert_eq!(item.title, "Feature Delivery");
        assert_eq!(item.subtitle, "Fan-out workflow");
        assert_eq!(
            item.preview,
            "1/2 enabled · 0/1 enabled · planner · builder"
        );
        assert_eq!(item.source_label, "Bundled default");
        assert_eq!(item.status_label, "Completed");
        assert_eq!(item.status_tone, "sage");
        assert_eq!(item.page_url, "/workflows?workflow=feature%20dev%2Frouter");
        assert!(item.active);
    }

    #[test]
    fn build_workflow_detail_populates_meta_steps_automations_runs_and_trigger_controls() {
        let name = "feature dev/router";
        let long_result = "x".repeat(80);
        let mut entry = workflow_entry(name);
        entry.team = TeamDefinition {
            version: "1.0.0".into(),
            title: "Feature Delivery".into(),
            description: Some("Coordinate the launch plan.".into()),
            workflow: OrchestrationPattern::Router,
            agents: vec![
                team_agent("planner", Some("Plan features")),
                team_agent("builder", None),
            ],
            router: None,
            fan_out: None,
            goal: None,
            communication_mode: Default::default(),
        };
        entry.schedules = vec![
            schedule(
                "weekday-start",
                "0 8 * * 1-5",
                name,
                true,
                Some("2026-03-10 08:00:00"),
                Some("2026-03-11 08:00:00"),
            ),
            schedule("paused-window", "0 18 * * 1-5", name, false, None, None),
        ];
        entry.triggers = vec![
            trigger(
                "merge-gate",
                "pull_request_merged",
                "{\"branch\":\"main\"}",
                name,
                true,
                Some("2026-03-10 07:30:00"),
                3,
            ),
            trigger(
                "manual-override",
                "manual_event",
                "{\"kind\":\"manual\"}",
                name,
                false,
                None,
                0,
            ),
        ];
        entry.recent_runs = vec![
            run(
                "run/1 active",
                name,
                RunStatus::Suspended,
                2,
                5,
                Some(&long_result),
                "2026-03-10 10:00:00",
            ),
            run(
                "run 2",
                name,
                RunStatus::Running,
                1,
                3,
                None,
                "2026-03-10 11:00:00",
            ),
        ];

        let detail = build_workflow_detail(&entry).unwrap();

        assert_eq!(detail.title, "Feature Delivery");
        assert_eq!(detail.subtitle, "Coordinate the launch plan.");
        assert_eq!(detail.source_label, "Bundled default");
        assert_eq!(detail.status_label, "Suspended");
        assert_eq!(detail.status_tone, "amber");
        assert_eq!(meta_value(&detail, "Pattern"), "Router");
        assert_eq!(meta_value(&detail, "Agents"), "2");
        assert_eq!(meta_value(&detail, "Schedules"), "1/2 enabled");
        assert_eq!(meta_value(&detail, "Triggers"), "1/2 enabled");
        assert_eq!(
            meta_value(&detail, "Last run"),
            "Suspended · 2026-03-10 10:00:00"
        );
        assert_eq!(
            meta_value(&detail, "Automation"),
            "1/2 enabled · 1/2 enabled"
        );

        assert_eq!(detail.steps.len(), 2);
        assert_eq!(detail.steps[0].title, "Route 1 · planner");
        assert_eq!(detail.steps[0].detail, "Plan features");
        assert_eq!(detail.steps[0].badge, "Candidate");
        assert_eq!(detail.steps[0].badge_tone, "sage");
        assert_eq!(detail.steps[1].title, "Route 2 · builder");
        assert_eq!(detail.steps[1].detail, "No role description provided.");
        assert_eq!(detail.steps[1].badge, "Candidate");

        assert_eq!(detail.automations.len(), 4);
        assert_eq!(detail.automations[0].kind, "Schedule");
        assert_eq!(detail.automations[0].title, "weekday-start");
        assert_eq!(
            detail.automations[0].detail,
            "0 8 * * 1-5 · team feature dev/router"
        );
        assert_eq!(
            detail.automations[0].note,
            "Last 2026-03-10 08:00:00 · Next 2026-03-11 08:00:00"
        );
        assert_eq!(detail.automations[0].status_label, "Enabled");
        assert_eq!(detail.automations[0].status_tone, "sage");
        assert_eq!(detail.automations[1].status_label, "Paused");
        assert_eq!(detail.automations[1].note, "No executions recorded yet.");
        assert_eq!(detail.automations[2].kind, "Trigger");
        assert_eq!(
            detail.automations[2].detail,
            "pull request merged · {\"branch\":\"main\"}"
        );
        assert_eq!(
            detail.automations[2].note,
            "Last fired 2026-03-10 07:30:00 · 3 total fire(s)"
        );
        assert_eq!(detail.automations[2].status_label, "Enabled");
        assert_eq!(detail.automations[3].status_label, "Paused");
        assert_eq!(detail.automations[3].note, "0 total fire(s)");

        assert_eq!(detail.recent_runs.len(), 2);
        assert_eq!(detail.recent_runs[0].title, "Run run/1 active");
        assert_eq!(
            detail.recent_runs[0].detail,
            format!("2/5 steps · {}…", "x".repeat(72))
        );
        assert_eq!(detail.recent_runs[0].updated_at, "2026-03-10 10:00:00");
        assert_eq!(detail.recent_runs[0].status_label, "Suspended");
        assert_eq!(detail.recent_runs[0].status_tone, "amber");
        assert_eq!(detail.recent_runs[0].page_url, "/runs?run=run%2F1%20active");
        assert_eq!(detail.recent_runs[1].detail, "1/3 steps · Still executing");
        assert_eq!(detail.recent_runs[1].status_label, "Running");
        assert_eq!(detail.recent_runs[1].status_tone, "cyan");
        assert_eq!(detail.recent_runs[1].page_url, "/runs?run=run%202");

        assert!(detail.yaml.contains("title: Feature Delivery"));
        assert_eq!(
            detail.trigger_api_url,
            "/workflows/feature%20dev%2Frouter/trigger"
        );
        assert_eq!(
            detail.trigger_input,
            "Manual run requested from the web dashboard for feature dev/router"
        );
    }

    #[test]
    fn build_workflow_detail_uses_empty_state_defaults() {
        let detail = build_workflow_detail(&workflow_entry("alpha")).unwrap();

        assert_eq!(detail.subtitle, "No workflow description provided.");
        assert_eq!(detail.status_label, "Manual");
        assert_eq!(detail.status_tone, "neutral");
        assert_eq!(meta_value(&detail, "Pattern"), "Chain");
        assert_eq!(meta_value(&detail, "Agents"), "1");
        assert_eq!(meta_value(&detail, "Schedules"), "0 configured");
        assert_eq!(meta_value(&detail, "Triggers"), "0 configured");
        assert_eq!(meta_value(&detail, "Last run"), "No persisted runs yet.");
        assert_eq!(meta_value(&detail, "Automation"), "Manual only");
        assert_eq!(detail.steps[0].detail, "No role description provided.");
        assert!(detail.automations.is_empty());
        assert!(detail.recent_runs.is_empty());
        assert_eq!(detail.trigger_api_url, "/workflows/alpha/trigger");
        assert_eq!(
            detail.trigger_input,
            "Manual run requested from the web dashboard for alpha"
        );
    }

    #[test]
    fn build_workflow_automations_formats_history_variants() {
        let mut entry = workflow_entry("alpha");
        entry.schedules = vec![
            schedule(
                "nightly",
                "0 0 * * *",
                "alpha",
                true,
                Some("2026-03-10 00:00:00"),
                Some("2026-03-11 00:00:00"),
            ),
            schedule(
                "last-only",
                "0 6 * * *",
                "alpha",
                true,
                Some("2026-03-10 06:00:00"),
                None,
            ),
            schedule(
                "next-only",
                "0 12 * * *",
                "alpha",
                false,
                None,
                Some("2026-03-10 12:00:00"),
            ),
            schedule("never-ran", "0 18 * * *", "alpha", false, None, None),
        ];
        entry.triggers = vec![
            trigger(
                "github-push",
                "webhook_received",
                "{\"event\":\"push\"}",
                "alpha",
                true,
                Some("2026-03-10 01:00:00"),
                2,
            ),
            trigger(
                "manual-review",
                "manual_event",
                "{\"kind\":\"review\"}",
                "alpha",
                false,
                None,
                0,
            ),
        ];

        let automations = build_workflow_automations(&entry);

        assert_eq!(automations.len(), 6);
        assert_eq!(
            automations[0].note,
            "Last 2026-03-10 00:00:00 · Next 2026-03-11 00:00:00"
        );
        assert_eq!(automations[1].note, "Last 2026-03-10 06:00:00");
        assert_eq!(automations[2].note, "Next 2026-03-10 12:00:00");
        assert_eq!(automations[2].status_label, "Paused");
        assert_eq!(automations[3].note, "No executions recorded yet.");
        assert_eq!(
            automations[4].note,
            "Last fired 2026-03-10 01:00:00 · 2 total fire(s)"
        );
        assert_eq!(automations[4].status_label, "Enabled");
        assert_eq!(automations[5].note, "0 total fire(s)");
        assert_eq!(automations[5].status_label, "Paused");
    }
}
