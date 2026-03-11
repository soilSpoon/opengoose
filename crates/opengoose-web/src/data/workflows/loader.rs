use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, Schedule, ScheduleStore, Trigger, TriggerStore,
};
use opengoose_teams::{TeamStore, all_defaults as default_teams};

use super::catalog::TeamCatalogEntry;

pub(super) struct WorkflowPageData {
    pub(super) teams: Vec<TeamCatalogEntry>,
    pub(super) schedules: Vec<Schedule>,
    pub(super) triggers: Vec<Trigger>,
    pub(super) recent_runs: Vec<OrchestrationRun>,
    pub(super) using_preview: bool,
}

pub(super) fn load_workflow_page_data(db: Arc<Database>) -> Result<WorkflowPageData> {
    let teams = load_teams_catalog()?;
    let schedules = ScheduleStore::new(db.clone()).list()?;
    let triggers = TriggerStore::new(db.clone()).list()?;
    let recent_runs = OrchestrationStore::new(db).list_runs(None, 200)?;
    let using_preview = uses_preview_data(&teams, &schedules, &triggers, &recent_runs);

    Ok(WorkflowPageData {
        teams,
        schedules,
        triggers,
        recent_runs,
        using_preview,
    })
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

fn uses_preview_data(
    teams: &[TeamCatalogEntry],
    schedules: &[Schedule],
    triggers: &[Trigger],
    recent_runs: &[OrchestrationRun],
) -> bool {
    teams.iter().all(|team| !team.is_live)
        && schedules.is_empty()
        && triggers.is_empty()
        && recent_runs.is_empty()
}

#[cfg(test)]
mod tests {
    use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition};

    use super::*;

    fn minimal_team(title: &str, is_live: bool) -> TeamCatalogEntry {
        TeamCatalogEntry {
            name: title.into(),
            team: TeamDefinition {
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
                goal: None,
            },
            source_label: "Bundled default".into(),
            is_live,
        }
    }

    #[test]
    fn uses_preview_data_requires_seeded_catalog_only() {
        assert!(uses_preview_data(
            &[minimal_team("preview", false)],
            &[],
            &[],
            &[]
        ));
        assert!(!uses_preview_data(
            &[minimal_team("live", true)],
            &[],
            &[],
            &[]
        ));
    }
}
