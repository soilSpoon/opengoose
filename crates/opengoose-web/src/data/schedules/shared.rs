use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, Schedule, ScheduleStore,
};
use opengoose_teams::TeamStore;

pub(super) const NEW_SCHEDULE_KEY: &str = "__new__";

pub struct ScheduleSaveInput {
    pub original_name: Option<String>,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
}

pub(super) struct ScheduleCatalog {
    pub(super) schedules: Vec<Schedule>,
    pub(super) runs: Vec<OrchestrationRun>,
    pub(super) installed_teams: Vec<String>,
}

pub(super) enum Selection {
    Existing(String),
    New,
}

pub(super) struct ScheduleDraft {
    pub(super) original_name: Option<String>,
    pub(super) name: String,
    pub(super) cron_expression: String,
    pub(super) team_name: String,
    pub(super) input: String,
    pub(super) enabled: bool,
}

pub(super) fn load_catalog(db: Arc<Database>) -> Result<ScheduleCatalog> {
    let schedules = ScheduleStore::new(db.clone()).list()?;
    let runs = OrchestrationStore::new(db).list_runs(None, 200)?;
    let installed_teams = load_installed_team_names()?;
    Ok(ScheduleCatalog {
        schedules,
        runs,
        installed_teams,
    })
}

pub(super) fn resolve_selection(catalog: &ScheduleCatalog, selected: Option<String>) -> Selection {
    match selected.as_deref() {
        Some(NEW_SCHEDULE_KEY) => Selection::New,
        Some(target)
            if catalog
                .schedules
                .iter()
                .any(|schedule| schedule.name == target) =>
        {
            Selection::Existing(target.to_string())
        }
        _ => catalog
            .schedules
            .first()
            .map(|schedule| Selection::Existing(schedule.name.clone()))
            .unwrap_or(Selection::New),
    }
}

fn load_installed_team_names() -> Result<Vec<String>> {
    let mut names = TeamStore::new()?.list()?;
    names.sort();
    Ok(names)
}
