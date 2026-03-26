// OpenGoose v0.2 — CLI entry point
//
// Routes to TUI (default), headless mode, or CLI subcommands.
// Board + Goose Agent wiring lives in runtime.rs.

mod cli;
mod commands;
mod headless;
mod logs;
mod runtime;
mod skills;
mod tui;
mod web;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, RunMode};

// Re-export setup helpers so sibling modules can use `crate::home_dir()` / `crate::db_url()`.
pub(crate) use cli::setup::{db_url, home_dir};

/// Global mutex for tests that modify environment variables (HOME, XDG_STATE_HOME, cwd).
/// All such tests across every module must acquire this lock to avoid cross-contamination.
#[cfg(test)]
pub(crate) use cli::setup::ENV_LOCK;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let run_mode = match &cli.command {
        None => RunMode::Tui,
        Some(Commands::Run { .. }) => RunMode::Headless,
        _ => RunMode::CliSubcommand,
    };
    let log_rx = cli::setup_logging(run_mode)?;

    cli::commands::dispatch(cli, log_rx).await
}
