use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;

use crate::cmd::output::CliOutput;
use opengoose_projects::ProjectStore;

mod add;
mod init;
mod list;
mod remove;
mod run;
mod show;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose project list\n  opengoose project show opengoose-dev\n  opengoose project add ./opengoose-project.yaml\n  opengoose project run opengoose-dev \"fix the login bug\" --team code-review\n  opengoose --json project list"
)]
/// Subcommands for `opengoose project`.
pub enum ProjectAction {
    /// List all project definitions
    #[command(after_help = "Examples:\n  opengoose project list\n  opengoose --json project list")]
    List,
    /// Show a project's full YAML
    #[command(after_help = "Example:\n  opengoose project show opengoose-dev")]
    Show {
        /// Project name (e.g. opengoose-dev)
        name: String,
    },
    /// Add a project from a YAML file
    #[command(after_help = "Example:\n  opengoose project add ./opengoose-project.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the project already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a project
    #[command(after_help = "Example:\n  opengoose project remove opengoose-dev")]
    Remove {
        /// Project name (e.g. opengoose-dev)
        name: String,
    },
    /// Create a sample project YAML in the current directory
    #[command(after_help = "Examples:\n  opengoose project init\n  opengoose project init --force")]
    Init {
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    /// Run a project's default team with the given input
    #[command(
        after_help = "Examples:\n  opengoose project run opengoose-dev \"fix the login bug\"\n  opengoose project run opengoose-dev \"review the PR\" --team code-review"
    )]
    Run {
        /// Project name (e.g. opengoose-dev)
        project: String,
        /// Input to the team workflow
        input: String,
        /// Team to run (defaults to the project's `default_team`)
        #[arg(long)]
        team: Option<String>,
    },
}

/// Dispatch and execute the selected project subcommand.
pub async fn execute(action: ProjectAction, output: CliOutput) -> Result<()> {
    let store = ProjectStore::new()?;
    execute_with_store(action, store, output).await
}

pub async fn execute_with_store(
    action: ProjectAction,
    store: ProjectStore,
    output: CliOutput,
) -> Result<()> {
    match action {
        ProjectAction::List => list::run(&store, output),
        ProjectAction::Show { name } => show::run(&name, &store, output),
        ProjectAction::Add { path, force } => add::run(&path, force, &store, output),
        ProjectAction::Remove { name } => remove::run(&name, &store, output),
        ProjectAction::Init { force } => init::run(force, output),
        ProjectAction::Run {
            project,
            input,
            team,
        } => run::run(&project, &input, team.as_deref(), &store).await,
    }
}
