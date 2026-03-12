use clap::Parser;

use crate::cli::{Cli, Command, CompletionShell};
use crate::cmd;
use crate::cmd::output::OutputMode;

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
