use std::sync::Arc;

use crate::error::CliResult;
use clap::Subcommand;

use opengoose_persistence::{Database, ScheduleStore};

mod commands;
mod logic;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
/// Subcommands for `opengoose schedule`.
pub enum ScheduleAction {
    /// Add a new cron schedule
    Add {
        /// Unique name for this schedule
        name: String,
        /// Cron expression (6-field: sec min hour day month weekday)
        #[arg(long)]
        cron: String,
        /// Team name to run
        #[arg(long)]
        team: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all schedules
    List,
    /// Remove a schedule
    Remove {
        /// Schedule name
        name: String,
    },
    /// Enable a schedule
    Enable {
        /// Schedule name
        name: String,
    },
    /// Disable a schedule
    Disable {
        /// Schedule name
        name: String,
    },
    /// Show status of a specific schedule
    Status {
        /// Schedule name
        name: String,
    },
}

/// Dispatch and execute the selected schedule subcommand.
pub fn execute(action: ScheduleAction) -> CliResult<()> {
    let db = Arc::new(Database::open()?);
    let team_store = opengoose_teams::TeamStore::new()?;
    run(action, db, &team_store)
}

/// Testable dispatch: accepts injected db and team_store.
pub(crate) fn run(
    action: ScheduleAction,
    db: Arc<Database>,
    team_store: &opengoose_teams::TeamStore,
) -> CliResult<()> {
    let store = ScheduleStore::new(db);
    match action {
        ScheduleAction::Add {
            name,
            cron,
            team,
            input,
        } => commands::add(&store, team_store, &name, &cron, &team, &input),
        ScheduleAction::List => commands::list(&store),
        ScheduleAction::Remove { name } => commands::remove(&store, &name),
        ScheduleAction::Enable { name } => commands::enable(&store, &name),
        ScheduleAction::Disable { name } => commands::disable(&store, &name),
        ScheduleAction::Status { name } => commands::status(&store, &name),
    }
}
