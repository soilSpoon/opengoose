use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, Schedule, ScheduleStore,
};
use opengoose_teams::TeamStore;

pub(super) struct ScheduleCatalog {
    pub(super) schedules: Vec<Schedule>,
    pub(super) runs: Vec<OrchestrationRun>,
    pub(super) installed_teams: Vec<String>,
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

fn load_installed_team_names() -> Result<Vec<String>> {
    let mut names = TeamStore::new()?.list()?;
    names.sort();
    Ok(names)
}
