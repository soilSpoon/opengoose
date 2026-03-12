use crate::error::{CliError, CliResult};

use crate::cli::{Cli, Command};
use crate::cmd::output::CliOutput;
use crate::dispatch;

/// Top-level orchestrator: initialise the crypto provider, set environment
/// variables that must happen before threads are spawned, build the tokio
/// runtime, and hand off to [`dispatch::dispatch`].
pub(crate) fn run(cli: Cli, output: CliOutput) -> CliResult<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|err| CliError::Validation(format!("failed to initialize rustls crypto provider: {err:?}")))?;

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
                return Err(CliError::Validation(format!("`opengoose run` does not support --json output")));
            }
            opengoose_core::setup_profiles_and_teams()?;
        }
        Command::Web { .. } => {
            if output.is_json() {
                return Err(CliError::Validation(format!("`opengoose web` does not support --json output")));
            }
            opengoose_core::setup_profiles_and_teams()?;
        }
        _ => {}
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(dispatch::dispatch(command, output))
}

fn runtime_model_override(command: &Command) -> Option<&str> {
    match command {
        Command::Run { model: Some(model) } => Some(model.as_str()),
        Command::Team {
            action:
                crate::cmd::team::TeamAction::Run {
                    model: Some(model), ..
                },
        } => Some(model.as_str()),
        _ => None,
    }
}
