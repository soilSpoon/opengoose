mod mutations;
mod queries;
#[cfg(test)]
mod tests;

pub use mutations::{delete_schedule, save_schedule, toggle_schedule};
pub use queries::load_schedules_page;

pub use self::types::ScheduleSaveInput;

mod types {
    use opengoose_persistence::{OrchestrationRun, Schedule};

    pub const NEW_SCHEDULE_KEY: &str = "__new__";

    pub struct ScheduleSaveInput {
        pub original_name: Option<String>,
        pub name: String,
        pub cron_expression: String,
        pub team_name: String,
        pub input: String,
        pub enabled: bool,
    }

    pub(super) struct ScheduleCatalog {
        pub schedules: Vec<Schedule>,
        pub runs: Vec<OrchestrationRun>,
        pub installed_teams: Vec<String>,
    }

    pub(super) enum Selection {
        Existing(String),
        New,
    }

    pub(super) struct ScheduleDraft {
        pub original_name: Option<String>,
        pub name: String,
        pub cron_expression: String,
        pub team_name: String,
        pub input: String,
        pub enabled: bool,
    }
}

pub(self) use types::*;
