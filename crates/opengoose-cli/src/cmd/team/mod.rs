use std::path::PathBuf;

use crate::error::CliResult;
use clap::Subcommand;

use crate::cmd::output::CliOutput;
use opengoose_teams::TeamStore;

mod add;
mod init;
mod list;
mod logs;
mod remove;
mod render;
mod resume;
mod run;
mod show;
mod status;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose team list\n  opengoose team show code-review\n  opengoose --json team list"
)]
/// Subcommands for `opengoose team`.
pub enum TeamAction {
    /// List all team definitions
    #[command(after_help = "Examples:\n  opengoose team list\n  opengoose --json team list")]
    List,
    /// Show a team's full YAML
    #[command(after_help = "Example:\n  opengoose team show code-review")]
    Show {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Add a team from a YAML file
    #[command(after_help = "Example:\n  opengoose team add ./teams/custom.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the team already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a team
    #[command(after_help = "Example:\n  opengoose team remove code-review")]
    Remove {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Install bundled default teams
    #[command(after_help = "Examples:\n  opengoose team init\n  opengoose team init --force")]
    Init {
        /// Overwrite existing teams
        #[arg(long)]
        force: bool,
    },
    /// Run a team workflow
    Run {
        /// Team name (e.g. code-review)
        team: String,
        /// Input to the team workflow
        input: String,
        /// Override the model for this team run
        #[arg(long)]
        model: Option<String>,
    },
    /// Show status of a team run
    Status {
        /// Run ID (omit to list recent runs)
        run_id: Option<String>,
    },
    /// Show logs for a team run
    Logs {
        /// Run ID
        run_id: String,
    },
    /// Resume a suspended team run
    Resume {
        /// Run ID
        run_id: String,
    },
}

/// Dispatch and execute the selected team subcommand.
pub async fn execute(action: TeamAction, output: CliOutput) -> CliResult<()> {
    let store = TeamStore::new()?;
    execute_with_store(action, store, output).await
}

pub async fn execute_with_store(
    action: TeamAction,
    store: TeamStore,
    output: CliOutput,
) -> CliResult<()> {
    match action {
        TeamAction::List => list::run(&store, output),
        TeamAction::Show { name } => show::run(&name, &store, output),
        TeamAction::Add { path, force } => add::run(&path, force, &store, output),
        TeamAction::Remove { name } => remove::run(&name, &store, output),
        TeamAction::Init { force } => init::run(force, &store, output),
        TeamAction::Run { team, input, model } => run::run(&team, &input, model).await,
        TeamAction::Status { run_id } => status::run(run_id.as_deref()),
        TeamAction::Logs { run_id } => logs::run(&run_id),
        TeamAction::Resume { run_id } => resume::run(&run_id).await,
    }
}
