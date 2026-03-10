mod cmd;

use anyhow::Result;
use clap::Parser;

/// OpenGoose — Goose-native multi-channel AI orchestrator
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
    /// Manage skill packages (named extension bundles)
    Skill {
        #[command(subcommand)]
        action: cmd::skill::SkillAction,
    },
    /// Manage team definitions
    Team {
        #[command(subcommand)]
        action: cmd::team::TeamAction,
    },
    /// Manage cron schedules for automatic team execution
    Schedule {
        #[command(subcommand)]
        action: cmd::schedule::ScheduleAction,
    },
    /// Send and inspect inter-agent messages
    Message {
        #[command(subcommand)]
        action: cmd::message::MessageAction,
    },
    /// Start the web dashboard server
    Web {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
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
    if matches!(
        cli.command,
        None | Some(Command::Run) | Some(Command::Web { .. })
    ) {
        opengoose_core::setup_profiles_and_teams()?;
    }

    // Now build the tokio runtime manually so worker threads start after env setup.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        match cli.command {
            None | Some(Command::Run) => cmd::run::execute().await,
            Some(Command::Auth { action }) => cmd::auth::execute(action).await,
            Some(Command::Profile { action }) => cmd::profile::execute(action),
            Some(Command::Skill { action }) => cmd::skill::execute(action),
            Some(Command::Team { action }) => cmd::team::execute(action).await,
            Some(Command::Schedule { action }) => cmd::schedule::execute(action),
            Some(Command::Message { action }) => cmd::message::execute(action).await,
            Some(Command::Web { port }) => cmd::web::execute(port).await,
        }
    })
}
