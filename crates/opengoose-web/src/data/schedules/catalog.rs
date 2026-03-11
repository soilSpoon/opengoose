use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, OrchestrationStore, ScheduleStore};
use opengoose_teams::TeamStore;

use super::{NEW_SCHEDULE_KEY, ScheduleCatalog, Selection};

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
