use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::generate;

/// OpenGoose — Goose-native multi-channel AI orchestrator
#[derive(Parser)]
#[command(
    name = "opengoose",
    version,
    about,
    after_help = "Examples:\n  opengoose\n  opengoose auth list\n  opengoose --json profile list\n  opengoose db cleanup --profile main\n  opengoose completion zsh > ~/.zsh/completions/_opengoose"
)]
pub(crate) struct Cli {
    /// Emit machine-readable JSON for supported commands
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand)]
pub(crate) enum Command {
    /// Start the gateway and TUI (default when no subcommand is given)
    #[command(after_help = "Example:\n  opengoose run")]
    Run {
        /// Override the default Goose model for this runtime
        #[arg(long)]
        model: Option<String>,
    },
    /// Manage AI provider authentication and credentials
    #[command(
        after_help = "Examples:\n  opengoose auth list\n  opengoose auth login openai\n  opengoose --json auth models anthropic"
    )]
    Auth {
        #[command(subcommand)]
        action: crate::cmd::auth::AuthAction,
    },
    /// Manage agent profiles
    #[command(
        after_help = "Examples:\n  opengoose profile init\n  opengoose profile show developer\n  opengoose profile set main --event-retention-days 14\n  opengoose --json profile list"
    )]
    Profile {
        #[command(subcommand)]
        action: crate::cmd::profile::ProfileAction,
    },
    /// Run database maintenance tasks
    #[command(
        after_help = "Examples:\n  opengoose db cleanup --profile main\n  opengoose db cleanup --retention-days 30 --event-retention-days 14\n  opengoose --json db cleanup --profile main"
    )]
    Db {
        #[command(subcommand)]
        action: crate::cmd::db::DbAction,
    },
    /// Inspect persisted event history and audit trails
    #[command(
        after_help = "Examples:\n  opengoose event history --limit 100\n  opengoose event history --filter gateway:discord --since 24h\n  opengoose --json event history --filter kind:message_received"
    )]
    Event {
        #[command(subcommand)]
        action: crate::cmd::event::EventAction,
    },
    /// Manage skill packages (named extension bundles)
    Skill {
        #[command(subcommand)]
        action: crate::cmd::skill::SkillAction,
    },
    /// Manage project definitions and run project workflows
    #[command(
        after_help = "Examples:\n  opengoose project init\n  opengoose project show opengoose-dev\n  opengoose project run opengoose-dev \"fix the bug\"\n  opengoose --json project list"
    )]
    Project {
        #[command(subcommand)]
        action: crate::cmd::project::ProjectAction,
    },
    /// Manage team definitions
    #[command(
        after_help = "Examples:\n  opengoose team init\n  opengoose team show code-review\n  opengoose --json team list"
    )]
    Team {
        #[command(subcommand)]
        action: crate::cmd::team::TeamAction,
    },
    /// Manage monitoring alert rules
    #[command(
        after_help = "Examples:\n  opengoose alert list\n  opengoose alert create high-backlog --metric queue_backlog --condition gt --threshold 100\n  opengoose alert test"
    )]
    Alert {
        #[command(subcommand)]
        action: crate::cmd::alert::AlertAction,
    },
    /// Manage API keys for web endpoint authentication
    #[command(
        name = "api-key",
        after_help = "Examples:\n  opengoose api-key generate --description \"CI pipeline\"\n  opengoose api-key list\n  opengoose api-key revoke <KEY_ID>"
    )]
    ApiKey {
        #[command(subcommand)]
        action: crate::cmd::api_key::ApiKeyAction,
    },
    /// Manage cron schedules for automatic team execution
    Schedule {
        #[command(subcommand)]
        action: crate::cmd::schedule::ScheduleAction,
    },
    /// Manage event triggers for automatic team execution
    Trigger {
        #[command(subcommand)]
        action: crate::cmd::trigger::TriggerAction,
    },
    /// Manage plugins (dynamic skill loaders and channel adapters)
    Plugin {
        #[command(subcommand)]
        action: crate::cmd::plugin::PluginAction,
    },
    /// Manage remote agent connections
    Remote {
        #[command(subcommand)]
        action: crate::cmd::remote::RemoteAction,
    },
    /// Send and inspect inter-agent messages
    Message {
        #[command(subcommand)]
        action: crate::cmd::message::MessageAction,
    },
    /// Start the web dashboard server
    Web {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Path to TLS certificate PEM file (enables HTTPS/WSS when provided with --tls-key)
        #[arg(long)]
        tls_cert: Option<std::path::PathBuf>,
        /// Path to TLS private key PEM file (enables HTTPS/WSS when provided with --tls-cert)
        #[arg(long)]
        tls_key: Option<std::path::PathBuf>,
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
pub(crate) enum CompletionShell {
    Bash,
    Zsh,
}

pub(crate) fn print_completion(shell: CompletionShell) {
    let shell = match shell {
        CompletionShell::Bash => clap_complete::Shell::Bash,
        CompletionShell::Zsh => clap_complete::Shell::Zsh,
    };

    let mut command = Cli::command();
    let mut stdout = std::io::stdout();
    generate(shell, &mut command, "opengoose", &mut stdout);
}
