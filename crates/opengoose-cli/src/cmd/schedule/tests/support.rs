use std::sync::Arc;

use opengoose_persistence::{Database, ScheduleStore};
use opengoose_teams::TeamStore;

pub(super) fn make_store() -> ScheduleStore {
    let db = Arc::new(Database::open_in_memory().unwrap());
    ScheduleStore::new(db)
}

pub(super) fn make_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

/// Create a temporary TeamStore with a named team YAML file.
pub(super) fn make_team_store_with(team_name: &str) -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        "version: \"1.0\"\ntitle: {team_name}\nworkflow: chain\nagents:\n  - profile: default\n"
    );
    std::fs::write(dir.path().join(format!("{team_name}.yaml")), yaml).unwrap();
    let store = TeamStore::with_dir(dir.path().to_path_buf());
    (dir, store)
}

pub(super) fn empty_team_store() -> (tempfile::TempDir, TeamStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = TeamStore::with_dir(dir.path().to_path_buf());
    (dir, store)
}
