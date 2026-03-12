use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;

use opengoose_persistence::{Database, TriggerStore};

mod commands;
mod logic;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
/// Subcommands for `opengoose trigger`.
pub enum TriggerAction {
    /// Add a new event trigger
    Add {
        /// Unique name for this trigger
        name: String,
        /// Trigger type (file_watch, message_received, schedule_complete, webhook_received)
        #[arg(long, name = "type")]
        trigger_type: String,
        /// Team name to run when the trigger fires
        #[arg(long)]
        team: String,
        /// JSON condition for matching (e.g. '{"channel":"alerts"}')
        #[arg(long, default_value = "{}")]
        condition: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all triggers
    List,
    /// Remove a trigger
    Remove {
        /// Trigger name
        name: String,
    },
    /// Enable a trigger
    Enable {
        /// Trigger name
        name: String,
    },
    /// Disable a trigger
    Disable {
        /// Trigger name
        name: String,
    },
    /// Show status of a specific trigger
    Status {
        /// Trigger name
        name: String,
    },
}

/// Dispatch and execute the selected trigger subcommand.
pub fn execute(action: TriggerAction) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let team_store = opengoose_teams::TeamStore::new()?;
    run(action, db, &team_store)
}

/// Testable dispatch: accepts injected db and team_store.
pub(crate) fn run(
    action: TriggerAction,
    db: Arc<Database>,
    team_store: &opengoose_teams::TeamStore,
) -> Result<()> {
    let store = TriggerStore::new(db);
    match action {
        TriggerAction::Add {
            name,
            trigger_type,
            team,
            condition,
            input,
        } => commands::add(
            &store,
            team_store,
            &name,
            &trigger_type,
            &team,
            &condition,
            &input,
        ),
        TriggerAction::List => commands::list(&store),
        TriggerAction::Remove { name } => commands::remove(&store, &name),
        TriggerAction::Enable { name } => commands::enable(&store, &name),
        TriggerAction::Disable { name } => commands::disable(&store, &name),
        TriggerAction::Status { name } => commands::status(&store, &name),
    }
}
