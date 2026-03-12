use std::path::PathBuf;

use crate::error::CliResult;
use clap::Subcommand;

use crate::cmd::output::CliOutput;

mod add;
mod init;
mod list;
mod remove;
mod set;
mod show;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose profile list\n  opengoose profile show developer\n  opengoose profile set main --message-retention-days 30\n  opengoose profile set main --event-retention-days 14\n  opengoose --json profile list"
)]
/// Subcommands for `opengoose profile`.
pub enum ProfileAction {
    /// List all agent profiles
    #[command(after_help = "Examples:\n  opengoose profile list\n  opengoose --json profile list")]
    List,
    /// Show a profile's full YAML
    #[command(after_help = "Example:\n  opengoose profile show developer")]
    Show {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Update configurable settings on an existing profile
    #[command(
        after_help = "Examples:\n  opengoose profile set main --message-retention-days 30\n  opengoose profile set main --event-retention-days 14\n  opengoose profile set main --clear-message-retention-days"
    )]
    Set {
        /// Profile name (e.g. main)
        name: String,
        /// Retain persisted session messages for N days
        #[arg(long, conflicts_with = "clear_message_retention_days")]
        message_retention_days: Option<u32>,
        /// Clear any configured message retention and keep messages forever
        #[arg(long, conflicts_with = "message_retention_days")]
        clear_message_retention_days: bool,
        /// Retain persisted event history for N days
        #[arg(long, conflicts_with = "clear_event_retention_days")]
        event_retention_days: Option<u32>,
        /// Clear any configured event retention and fall back to the runtime default
        #[arg(long, conflicts_with = "event_retention_days")]
        clear_event_retention_days: bool,
    },
    /// Add a profile from a YAML file
    #[command(after_help = "Example:\n  opengoose profile add ./profiles/custom.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the profile already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a profile
    #[command(after_help = "Example:\n  opengoose profile remove developer")]
    Remove {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Install bundled default profiles
    #[command(after_help = "Examples:\n  opengoose profile init\n  opengoose profile init --force")]
    Init {
        /// Overwrite existing profiles
        #[arg(long)]
        force: bool,
    },
}

/// Dispatch and execute the selected profile subcommand.
pub fn execute(action: ProfileAction, output: CliOutput) -> CliResult<()> {
    match action {
        ProfileAction::List => list::run(output),
        ProfileAction::Show { name } => show::run(&name, output),
        ProfileAction::Set {
            name,
            message_retention_days,
            clear_message_retention_days,
            event_retention_days,
            clear_event_retention_days,
        } => set::run(
            &name,
            message_retention_days,
            clear_message_retention_days,
            event_retention_days,
            clear_event_retention_days,
            output,
        ),
        ProfileAction::Add { path, force } => add::run(&path, force, output),
        ProfileAction::Remove { name } => remove::run(&name, output),
        ProfileAction::Init { force } => init::run(force, output),
    }
}
