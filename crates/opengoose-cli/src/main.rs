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
    after_help = "Examples:\n  opengoose\n  opengoose auth list\n  opengoose --json profile list\n  opengoose db cleanup --profile main\n  opengoose completion zsh > ~/.zsh/completions/_opengoose"
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
        action: cmd::auth::AuthAction,
    },
    /// Manage agent profiles
    #[command(
        after_help = "Examples:\n  opengoose profile init\n  opengoose profile show developer\n  opengoose profile set main --event-retention-days 14\n  opengoose --json profile list"
    )]
    Profile {
        #[command(subcommand)]
        action: cmd::profile::ProfileAction,
    },
    /// Run database maintenance tasks
    #[command(
        after_help = "Examples:\n  opengoose db cleanup --profile main\n  opengoose db cleanup --retention-days 30 --event-retention-days 14\n  opengoose --json db cleanup --profile main"
    )]
    Db {
        #[command(subcommand)]
        action: cmd::db::DbAction,
    },
    /// Inspect persisted event history and audit trails
    #[command(
        after_help = "Examples:\n  opengoose event history --limit 100\n  opengoose event history --filter gateway:discord --since 24h\n  opengoose --json event history --filter kind:message_received"
    )]
    Event {
        #[command(subcommand)]
        action: cmd::event::EventAction,
    },
    /// Manage skill packages (named extension bundles)
    Skill {
        #[command(subcommand)]
        action: cmd::skill::SkillAction,
    },
    /// Manage project definitions and run project workflows
    #[command(
        after_help = "Examples:\n  opengoose project init\n  opengoose project show opengoose-dev\n  opengoose project run opengoose-dev \"fix the bug\"\n  opengoose --json project list"
    )]
    Project {
        #[command(subcommand)]
        action: cmd::project::ProjectAction,
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
    /// Manage API keys for web endpoint authentication
    #[command(
        name = "api-key",
        after_help = "Examples:\n  opengoose api-key generate --description \"CI pipeline\"\n  opengoose api-key list\n  opengoose api-key revoke <KEY_ID>"
    )]
    ApiKey {
        #[command(subcommand)]
        action: cmd::api_key::ApiKeyAction,
    },
    /// Manage cron schedules for automatic team execution
    Schedule {
        #[command(subcommand)]
        action: cmd::schedule::ScheduleAction,
    },
    /// Manage event triggers for automatic team execution
    Trigger {
        #[command(subcommand)]
        action: cmd::trigger::TriggerAction,
    },
    /// Manage plugins (dynamic skill loaders and channel adapters)
    Plugin {
        #[command(subcommand)]
        action: cmd::plugin::PluginAction,
    },
    /// Manage remote agent connections
    Remote {
        #[command(subcommand)]
        action: cmd::remote::RemoteAction,
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

    let command = cli.command.unwrap_or(Command::Run { model: None });

    if let Some(model) = runtime_model_override(&command) {
        // Safety: this happens before the tokio runtime is started, matching the
        // same single-threaded env-var setup constraints as profile registration.
        unsafe {
            std::env::set_var("GOOSE_MODEL", model);
        }
    }

    // Set up profiles and env vars *before* spawning any threads.
    // `register_profiles_path` uses `unsafe { set_var }` which requires
    // single-threaded execution.
    match &command {
        Command::Run { .. } => {
            if output.is_json() {
                bail!("`opengoose run` does not support --json output");
            }
            opengoose_core::setup_profiles_and_teams()?;
        }
        Command::Web { .. } => {
            if output.is_json() {
                bail!("`opengoose web` does not support --json output");
            }
            opengoose_core::setup_profiles_and_teams()?;
        }
        _ => {}
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        match command {
            Command::Run { .. } => cmd::run::execute().await,
            Command::Auth { action } => cmd::auth::execute(action, output).await,
            Command::Profile { action } => cmd::profile::execute(action, output),
            Command::Db { action } => cmd::db::execute(action, output),
            Command::Event { action } => cmd::event::execute(action, output),
            Command::Skill { action } => cmd::skill::execute(action),
            Command::Project { action } => cmd::project::execute(action, output).await,
            Command::Team { action } => cmd::team::execute(action, output).await,
            Command::Alert { action } => cmd::alert::execute(action),
            Command::ApiKey { action } => cmd::api_key::execute(action, output),
            Command::Schedule { action } => cmd::schedule::execute(action),
            Command::Trigger { action } => cmd::trigger::execute(action),
            Command::Plugin { action } => cmd::plugin::execute(action),
            Command::Remote { action } => cmd::remote::execute(action).await,
            Command::Message { action } => cmd::message::execute(action).await,
            Command::Web { port, tls_cert, tls_key } => cmd::web::execute(port, tls_cert, tls_key).await,
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

fn runtime_model_override(command: &Command) -> Option<&str> {
    match command {
        Command::Run { model: Some(model) } => Some(model.as_str()),
        Command::Team {
            action:
                cmd::team::TeamAction::Run {
                    model: Some(model), ..
                },
        } => Some(model.as_str()),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_no_args_defaults_to_none_command() {
        let cli = Cli::parse_from(["opengoose"]);
        assert!(!cli.json);
        assert!(cli.command.is_none());
    }

    #[test]
    fn parse_json_flag_global() {
        let cli = Cli::parse_from(["opengoose", "--json", "profile", "list"]);
        assert!(cli.json);
    }

    #[test]
    fn parse_json_flag_after_subcommand() {
        let cli = Cli::parse_from(["opengoose", "profile", "--json", "list"]);
        assert!(cli.json);
    }

    #[test]
    fn parse_db_cleanup_subcommand() {
        let cli = Cli::parse_from(["opengoose", "db", "cleanup", "--profile", "main"]);
        assert!(matches!(
            cli.command,
            Some(Command::Db {
                action: cmd::db::DbAction::Cleanup { .. }
            })
        ));
    }

    #[test]
    fn parse_event_history_subcommand() {
        let cli = Cli::parse_from([
            "opengoose",
            "event",
            "history",
            "--filter",
            "gateway:discord",
            "--since",
            "24h",
        ]);
        assert!(matches!(
            cli.command,
            Some(Command::Event {
                action: cmd::event::EventAction::History { .. }
            })
        ));
    }

    #[test]
    fn parse_run_subcommand() {
        let cli = Cli::parse_from(["opengoose", "run"]);
        assert!(matches!(cli.command, Some(Command::Run { model: None })));
    }

    #[test]
    fn parse_run_subcommand_with_model_override() {
        let cli = Cli::parse_from(["opengoose", "run", "--model", "gpt-5-mini"]);
        assert!(matches!(
            cli.command,
            Some(Command::Run {
                model: Some(ref model)
            }) if model == "gpt-5-mini"
        ));
    }

    #[test]
    fn parse_team_list_subcommand() {
        let cli = Cli::parse_from(["opengoose", "team", "list"]);
        assert!(matches!(
            cli.command,
            Some(Command::Team {
                action: cmd::team::TeamAction::List
            })
        ));
    }

    #[test]
    fn parse_team_show_subcommand() {
        let cli = Cli::parse_from(["opengoose", "team", "show", "code-review"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Show { name },
            }) => {
                assert_eq!(name, "code-review");
            }
            _ => panic!("expected Team show command"),
        }
    }

    #[test]
    fn parse_team_remove_subcommand() {
        let cli = Cli::parse_from(["opengoose", "team", "remove", "code-review"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Remove { name },
            }) => {
                assert_eq!(name, "code-review");
            }
            _ => panic!("expected Team remove command"),
        }
    }

    #[test]
    fn parse_json_flag_after_team_subcommand_for_show() {
        let cli = Cli::parse_from(["opengoose", "team", "--json", "show", "code-review"]);

        assert!(cli.json);
        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Show { name },
            }) => {
                assert_eq!(name, "code-review");
            }
            _ => panic!("expected Team show command"),
        }
    }

    #[test]
    fn parse_team_add_force_subcommand() {
        let cli = Cli::parse_from([
            "opengoose",
            "team",
            "add",
            "/tmp/custom-team.yaml",
            "--force",
        ]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Add { path, force },
            }) => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/custom-team.yaml"));
                assert!(force);
            }
            _ => panic!("expected Team add command"),
        }
    }

    #[test]
    fn parse_team_run_subcommand() {
        let cli = Cli::parse_from(["opengoose", "team", "run", "code-review", "Ship it"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Run { team, input, model },
            }) => {
                assert_eq!(team, "code-review");
                assert_eq!(input, "Ship it");
                assert!(model.is_none());
            }
            _ => panic!("expected Team run command"),
        }
    }

    #[test]
    fn parse_team_run_subcommand_with_model_override() {
        let cli = Cli::parse_from([
            "opengoose",
            "team",
            "run",
            "code-review",
            "Ship it",
            "--model",
            "claude-3-7-sonnet",
        ]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Run { model, .. },
            }) => {
                assert_eq!(model.as_deref(), Some("claude-3-7-sonnet"));
            }
            _ => panic!("expected Team run command"),
        }
    }

    #[test]
    fn parse_team_status_with_run_id() {
        let cli = Cli::parse_from(["opengoose", "team", "status", "run-123"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Status { run_id },
            }) => {
                assert_eq!(run_id.as_deref(), Some("run-123"));
            }
            _ => panic!("expected Team status command"),
        }
    }

    #[test]
    fn parse_team_status_without_run_id() {
        let cli = Cli::parse_from(["opengoose", "team", "status"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Status { run_id },
            }) => {
                assert!(run_id.is_none());
            }
            _ => panic!("expected Team status command"),
        }
    }

    #[test]
    fn parse_team_logs_subcommand() {
        let cli = Cli::parse_from(["opengoose", "team", "logs", "run-456"]);

        match cli.command {
            Some(Command::Team {
                action: cmd::team::TeamAction::Logs { run_id },
            }) => {
                assert_eq!(run_id, "run-456");
            }
            _ => panic!("expected Team logs command"),
        }
    }

    #[test]
    fn parse_message_send_directed_subcommand() {
        let cli = Cli::parse_from([
            "opengoose",
            "message",
            "send",
            "--from",
            "frontend",
            "--to",
            "backend",
            "hello there",
        ]);

        match cli.command {
            Some(Command::Message {
                action:
                    cmd::message::MessageAction::Send {
                        from,
                        to,
                        channel,
                        payload,
                        session,
                    },
            }) => {
                assert_eq!(from, "frontend");
                assert_eq!(to.as_deref(), Some("backend"));
                assert!(channel.is_none());
                assert_eq!(payload, "hello there");
                assert_eq!(session, "cli:local:default");
            }
            _ => panic!("expected Message send command"),
        }
    }

    #[test]
    fn parse_message_send_channel_subcommand() {
        let cli = Cli::parse_from([
            "opengoose",
            "message",
            "send",
            "--from",
            "frontend",
            "--channel",
            "triage",
            "hello channel",
        ]);

        match cli.command {
            Some(Command::Message {
                action:
                    cmd::message::MessageAction::Send {
                        from,
                        to,
                        channel,
                        payload,
                        session,
                    },
            }) => {
                assert_eq!(from, "frontend");
                assert!(to.is_none());
                assert_eq!(channel.as_deref(), Some("triage"));
                assert_eq!(payload, "hello channel");
                assert_eq!(session, "cli:local:default");
            }
            _ => panic!("expected Message send command"),
        }
    }

    #[test]
    fn parse_message_list_filters() {
        let cli = Cli::parse_from([
            "opengoose",
            "message",
            "list",
            "--session",
            "cli:test:session",
            "--limit",
            "5",
            "--agent",
            "backend",
        ]);

        match cli.command {
            Some(Command::Message {
                action:
                    cmd::message::MessageAction::List {
                        session,
                        limit,
                        agent,
                        channel,
                    },
            }) => {
                assert_eq!(session, "cli:test:session");
                assert_eq!(limit, 5);
                assert_eq!(agent.as_deref(), Some("backend"));
                assert!(channel.is_none());
            }
            _ => panic!("expected Message list command"),
        }
    }

    #[test]
    fn parse_message_subscribe_timeout() {
        let cli = Cli::parse_from([
            "opengoose",
            "message",
            "subscribe",
            "--channel",
            "ops",
            "--timeout",
            "30",
        ]);

        match cli.command {
            Some(Command::Message {
                action:
                    cmd::message::MessageAction::Subscribe {
                        channel,
                        agent,
                        timeout,
                    },
            }) => {
                assert_eq!(channel.as_deref(), Some("ops"));
                assert!(agent.is_none());
                assert_eq!(timeout, 30);
            }
            _ => panic!("expected Message subscribe command"),
        }
    }

    #[test]
    fn parse_message_pending_subcommand() {
        let cli = Cli::parse_from([
            "opengoose",
            "message",
            "pending",
            "frontend",
            "--session",
            "cli:test:pending",
        ]);

        match cli.command {
            Some(Command::Message {
                action: cmd::message::MessageAction::Pending { agent, session },
            }) => {
                assert_eq!(agent, "frontend");
                assert_eq!(session, "cli:test:pending");
            }
            _ => panic!("expected Message pending command"),
        }
    }

    #[test]
    fn parse_web_default_port() {
        let cli = Cli::parse_from(["opengoose", "web"]);
        match cli.command {
            Some(Command::Web {
                port,
                tls_cert,
                tls_key,
            }) => {
                assert_eq!(port, 8080);
                assert!(tls_cert.is_none());
                assert!(tls_key.is_none());
            }
            _ => panic!("expected Web command"),
        }
    }

    #[test]
    fn parse_web_custom_port() {
        let cli = Cli::parse_from(["opengoose", "web", "--port", "3000"]);
        match cli.command {
            Some(Command::Web { port, .. }) => assert_eq!(port, 3000),
            _ => panic!("expected Web command"),
        }
    }

    #[test]
    fn parse_web_tls_flags() {
        let cli = Cli::parse_from([
            "opengoose",
            "web",
            "--tls-cert",
            "/etc/ssl/cert.pem",
            "--tls-key",
            "/etc/ssl/key.pem",
        ]);
        match cli.command {
            Some(Command::Web {
                tls_cert, tls_key, ..
            }) => {
                assert_eq!(tls_cert.unwrap().to_str().unwrap(), "/etc/ssl/cert.pem");
                assert_eq!(tls_key.unwrap().to_str().unwrap(), "/etc/ssl/key.pem");
            }
            _ => panic!("expected Web command"),
        }
    }

    #[test]
    fn parse_web_tls_cert_only_is_accepted_by_parser() {
        // Parser accepts --tls-cert without --tls-key; the validation happens in serve()
        let cli = Cli::parse_from(["opengoose", "web", "--tls-cert", "/etc/ssl/cert.pem"]);
        match cli.command {
            Some(Command::Web {
                tls_cert, tls_key, ..
            }) => {
                assert!(tls_cert.is_some());
                assert!(tls_key.is_none());
            }
            _ => panic!("expected Web command"),
        }
    }

    #[test]
    fn parse_completion_bash() {
        let cli = Cli::parse_from(["opengoose", "completion", "bash"]);
        match cli.command {
            Some(Command::Completion { shell }) => {
                assert!(matches!(shell, CompletionShell::Bash));
            }
            _ => panic!("expected Completion command"),
        }
    }

    #[test]
    fn parse_completion_zsh() {
        let cli = Cli::parse_from(["opengoose", "completion", "zsh"]);
        match cli.command {
            Some(Command::Completion { shell }) => {
                assert!(matches!(shell, CompletionShell::Zsh));
            }
            _ => panic!("expected Completion command"),
        }
    }

    #[test]
    fn parse_invalid_subcommand_fails() {
        let result = Cli::try_parse_from(["opengoose", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_flag_fails() {
        let result = Cli::try_parse_from(["opengoose", "--bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn default_command_is_run() {
        // Mirrors the unwrap_or in run(): None maps to Command::Run.
        let cli = Cli::parse_from(["opengoose"]);
        let command = cli.command.unwrap_or(Command::Run { model: None });
        assert!(matches!(command, Command::Run { model: None }));
    }

    #[test]
    fn output_mode_from_json_flag() {
        assert!(OutputMode::from_json_flag(true).is_json());
        assert!(!OutputMode::from_json_flag(false).is_json());
    }
}
