mod cmd;

use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::generate;

use cmd::output::{CliOutput, OutputMode, print_clap_error, print_error};

/// OpenGoose — Goose-native multi-channel AI orchestrator
#[derive(Parser)]
#[command(
    name = "opengoose",
    version,
    about,
    after_help = "Examples:\n  opengoose\n  opengoose auth list\n  opengoose --json profile list\n  opengoose completion zsh > ~/.zsh/completions/_opengoose"
)]
struct Cli {
    /// Emit machine-readable JSON for supported commands
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Start the gateway and TUI (default when no subcommand is given)
    #[command(after_help = "Example:\n  opengoose run")]
    Run,
    /// Manage AI provider authentication and credentials
    #[command(
        after_help = "Examples:\n  opengoose auth list\n  opengoose auth login openai\n  opengoose --json auth models anthropic"
    )]
    Auth {
        #[command(subcommand)]
        action: cmd::auth::AuthAction,
    },
    /// Manage agent profiles
    #[command(
        after_help = "Examples:\n  opengoose profile init\n  opengoose profile show developer\n  opengoose --json profile list"
    )]
    Profile {
        #[command(subcommand)]
        action: cmd::profile::ProfileAction,
    },
    /// Manage team definitions
    #[command(
        after_help = "Examples:\n  opengoose team init\n  opengoose team show code-review\n  opengoose --json team list"
    )]
    Team {
        #[command(subcommand)]
        action: cmd::team::TeamAction,
    },
    /// Manage monitoring alert rules
    #[command(
        after_help = "Examples:\n  opengoose alert list\n  opengoose alert create high-backlog --metric queue_backlog --condition gt --threshold 100\n  opengoose alert test"
    )]
    Alert {
        #[command(subcommand)]
        action: cmd::alert::AlertAction,
    },
    /// Generate shell completion scripts
    #[command(
        after_help = "Examples:\n  opengoose completion bash > ~/.local/share/bash-completion/completions/opengoose\n  opengoose completion zsh > ~/.zsh/completions/_opengoose"
    )]
    Completion {
        /// Shell to generate completions for
        shell: CompletionShell,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
}

fn main() -> ExitCode {
    let requested_json = std::env::args_os().any(|arg| arg == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return print_clap_error(requested_json, err),
    };

    let output = CliOutput::new(OutputMode::from_json_flag(cli.json));
    if let Err(err) = run(cli, output) {
        print_error(output, &err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run(cli: Cli, output: CliOutput) -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|err| anyhow::anyhow!("failed to initialize rustls crypto provider: {err:?}"))?;

    let command = cli.command.unwrap_or(Command::Run);
    if matches!(command, Command::Run) {
        if output.is_json() {
            bail!("`opengoose run` does not support --json output");
        }

        opengoose_core::setup_profiles_and_teams()?;
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        match command {
            Command::Run => cmd::run::execute().await,
            Command::Auth { action } => cmd::auth::execute(action, output).await,
            Command::Profile { action } => cmd::profile::execute(action, output),
            Command::Team { action } => cmd::team::execute(action, output),
            Command::Alert { action } => cmd::alert::execute(action),
            Command::Completion { shell } => {
                if output.is_json() {
                    bail!("`opengoose completion` prints shell scripts directly and does not support --json");
                }

                print_completion(shell);
                Ok(())
            }
        }
    })
}

fn print_completion(shell: CompletionShell) {
    let shell = match shell {
        CompletionShell::Bash => clap_complete::Shell::Bash,
        CompletionShell::Zsh => clap_complete::Shell::Zsh,
    };

    let mut command = Cli::command();
    let mut stdout = std::io::stdout();
    generate(shell, &mut command, "opengoose", &mut stdout);
}
