use opengoose_persistence::{OrchestrationRun, Schedule, Trigger};
use opengoose_teams::TeamDefinition;

#[derive(Clone)]
pub(super) struct TeamCatalogEntry {
    pub(super) name: String,
    pub(super) team: TeamDefinition,
    pub(super) source_label: String,
    pub(super) is_live: bool,
}

#[derive(Clone)]
pub(super) struct WorkflowCatalogEntry {
    pub(super) name: String,
    pub(super) team: TeamDefinition,
    pub(super) source_label: String,
    pub(super) schedules: Vec<Schedule>,
    pub(super) triggers: Vec<Trigger>,
    pub(super) recent_runs: Vec<OrchestrationRun>,
}

pub(super) fn build_workflow_catalog(
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
