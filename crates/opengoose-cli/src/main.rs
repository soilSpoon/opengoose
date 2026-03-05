mod cmd;

use anyhow::Result;
use clap::Parser;

/// OpenGoose — Discord-to-Goose AI orchestrator
#[derive(Parser)]
#[command(name = "opengoose", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Start the gateway and TUI (default when no subcommand is given)
    Run,
    /// Manage AI provider authentication and credentials
    Auth {
        #[command(subcommand)]
        action: cmd::auth::AuthAction,
    },
    /// Manage agent profiles
    Profile {
        #[command(subcommand)]
        action: cmd::profile::ProfileAction,
    },
    /// Manage team definitions
    Team {
        #[command(subcommand)]
        action: cmd::team::TeamAction,
    },
}

fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    // Set up profiles and env vars *before* spawning any threads.
    // `register_profiles_path` uses `unsafe { set_var }` which requires
    // single-threaded execution.
    if matches!(cli.command, None | Some(Command::Run)) {
        opengoose_core::setup_profiles_and_teams()?;
    }

    // Now build the tokio runtime manually so worker threads start after env setup.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        match cli.command {
            None | Some(Command::Run) => cmd::run::execute().await,
            Some(Command::Auth { action }) => cmd::auth::execute(action),
            Some(Command::Profile { action }) => cmd::profile::execute(action),
            Some(Command::Team { action }) => cmd::team::execute(action),
        }
    })
}
