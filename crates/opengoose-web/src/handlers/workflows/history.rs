use opengoose_persistence::{OrchestrationRun, Schedule, Trigger};
use opengoose_teams::{OrchestrationPattern, TeamDefinition};

use crate::handlers::AppError;
use crate::state::AppState;

use super::{WorkflowAutomation, WorkflowDetail, WorkflowItem, WorkflowRun, WorkflowStep};

pub(super) fn build_workflow_item(
    name: &str,
    team: &TeamDefinition,
    schedules: &[Schedule],
    triggers: &[Trigger],
    runs: &[OrchestrationRun],
) -> WorkflowItem {
    let last_run = runs.iter().find(|run| run.team_name == name);
    let workflow_schedules: Vec<_> = schedules
        .iter()
        .filter(|schedule| schedule.team_name == name)
        .collect();
    let workflow_triggers: Vec<_> = triggers
        .iter()
        .filter(|trigger| trigger.team_name == name)
        .collect();

    WorkflowItem {
        name: name.to_string(),
        title: team.title.clone(),
        description: team.description.clone(),
        workflow: workflow_name(team),
        agent_count: team.agents.len(),
        schedule_count: workflow_schedules.len(),
        enabled_schedule_count: workflow_schedules
            .iter()
            .filter(|schedule| schedule.enabled)
            .count(),
        trigger_count: workflow_triggers.len(),
        enabled_trigger_count: workflow_triggers
            .iter()
            .filter(|trigger| trigger.enabled)
            .count(),
        last_run_status: last_run.map(|run| status_label(run.status.as_str())),
        last_run_at: last_run.map(|run| run.updated_at.clone()),
    }
}

pub(super) fn build_workflow_detail(
    name: &str,
    team: &TeamDefinition,
    schedules: &[Schedule],
    triggers: &[Trigger],
    runs: &[OrchestrationRun],
    state: &AppState,
) -> Result<WorkflowDetail, AppError> {
    let automations = schedules
        .iter()
        .filter(|schedule| schedule.team_name == name)
        .map(|schedule| WorkflowAutomation {
            kind: "schedule",
            name: schedule.name.clone(),
            enabled: schedule.enabled,
            detail: schedule.cron_expression.clone(),
            note: schedule_note(schedule),
        })
        .chain(
            triggers
                .iter()
                .filter(|trigger| trigger.team_name == name)
                .map(|trigger| WorkflowAutomation {
                    kind: "trigger",
                    name: trigger.name.clone(),
                    enabled: trigger.enabled,
                    detail: trigger.trigger_type.clone(),
                    note: trigger_note(trigger),
                }),
        )
        .collect();
    let recent_runs = runs
        .iter()
        .filter(|run| run.team_name == name)
        .take(6)
        .map(|run| WorkflowRun {
            team_run_id: run.team_run_id.clone(),
            status: status_label(run.status.as_str()),
            current_step: run.current_step,
            total_steps: run.total_steps,
            updated_at: run.updated_at.clone(),
        })
        .collect();

    Ok(WorkflowDetail {
        name: name.to_string(),
        title: team.title.clone(),
        description: team.description.clone(),
        workflow: workflow_name(team),
        source_label: state.team_store.dir().display().to_string(),
        yaml: team.to_yaml()?,
        steps: team
            .agents
            .iter()
            .map(|agent| WorkflowStep {
                profile: agent.profile.clone(),
                role: agent.role.clone(),
            })
            .collect(),
        automations,
        recent_runs,
    })
}

fn schedule_note(schedule: &Schedule) -> String {
    match (&schedule.last_run_at, &schedule.next_run_at) {
        (Some(last_run), Some(next_run)) => format!("last {last_run} · next {next_run}"),
        (Some(last_run), None) => format!("last {last_run}"),
        (None, Some(next_run)) => format!("next {next_run}"),
        (None, None) => "no executions recorded".into(),
    }
}

fn trigger_note(trigger: &Trigger) -> String {
    trigger
        .last_fired_at
        .as_ref()
        .map(|last_fired| {
            format!(
                "last fired {last_fired} · {} total fire(s)",
                trigger.fire_count
            )
        })
        .unwrap_or_else(|| format!("{} total fire(s)", trigger.fire_count))
}

pub(super) fn workflow_name(team: &TeamDefinition) -> String {
    match team.workflow {
        OrchestrationPattern::Chain => "chain".into(),
        OrchestrationPattern::FanOut => "fan-out".into(),
        OrchestrationPattern::Router => "router".into(),
    }
}

pub(super) fn status_label(value: &str) -> String {
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
