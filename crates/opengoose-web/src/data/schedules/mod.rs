mod catalog;
mod input;
mod operations;
#[cfg(test)]
mod tests;
mod view;

use opengoose_persistence::{OrchestrationRun, Schedule};

pub use operations::{delete_schedule, load_schedules_page, save_schedule, toggle_schedule};

const NEW_SCHEDULE_KEY: &str = "__new__";
const MAX_SCHEDULE_NAME_BYTES: usize = 128;
const MAX_CRON_EXPRESSION_BYTES: usize = 128;
const MAX_TEAM_NAME_BYTES: usize = 128;
const MAX_SCHEDULE_INPUT_BYTES: usize = 8 * 1024;

pub struct ScheduleSaveInput {
    pub original_name: Option<String>,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
}

struct ScheduleCatalog {
    schedules: Vec<Schedule>,
    runs: Vec<OrchestrationRun>,
    installed_teams: Vec<String>,
}

enum Selection {
    Existing(String),
    New,
}

struct ScheduleDraft {
    original_name: Option<String>,
    name: String,
    cron_expression: String,
    team_name: String,
    input: String,
    enabled: bool,
}
