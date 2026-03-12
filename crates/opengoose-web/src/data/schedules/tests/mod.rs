use std::sync::Arc;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, ScheduleStore};
use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};

use super::editor::normalize_input;
use super::selection::NEW_SCHEDULE_KEY;
use super::*;
use crate::test_support::with_temp_home as with_shared_temp_home;

mod catalog;
mod editor;
mod history;
mod lifecycle;

fn with_temp_home(test: impl FnOnce()) {
    with_shared_temp_home("opengoose-schedules-home", test);
}

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("in-memory db should open"))
}

fn save_team(name: &str) {
    TeamStore::new()
        .expect("team store should open")
        .save(
            &TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: Some(format!("{name} team")),
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: "tester".into(),
                    role: Some("validate setup".into()),
                }],
                router: None,
                fan_out: None,
                goal: None,
            },
            true,
        )
        .expect("team should save");
}

fn new_schedule_input(name: &str) -> ScheduleSaveInput {
    ScheduleSaveInput {
        original_name: None,
        name: name.into(),
        cron_expression: "0 0 * * * *".into(),
        team_name: "ops".into(),
        input: String::new(),
        enabled: true,
    }
}

fn seed_schedule(db: Arc<Database>, name: &str) {
    save_schedule(db, new_schedule_input(name)).expect("seed schedule should save");
}
