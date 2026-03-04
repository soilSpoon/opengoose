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
    /// Manage stored secrets
    Secret {
        #[command(subcommand)]
        action: cmd::secret::SecretAction,
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

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    match cli.command {
        None | Some(Command::Run) => cmd::run::execute().await,
        Some(Command::Secret { action }) => cmd::secret::execute(action),
        Some(Command::Profile { action }) => cmd::profile::execute(action),
        Some(Command::Team { action }) => cmd::team::execute(action),
    }
}
