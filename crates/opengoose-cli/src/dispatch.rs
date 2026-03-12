use crate::error::{CliError, CliResult};

use crate::cli::{self, Command};
use crate::cmd;
use crate::cmd::output::CliOutput;

/// Dispatch a parsed [`Command`] to the appropriate subcommand handler.
pub(crate) async fn dispatch(command: Command, output: CliOutput) -> CliResult<()> {
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
        Command::Web {
            port,
            tls_cert,
            tls_key,
        } => cmd::web::execute(port, tls_cert, tls_key).await,
        Command::Completion { shell } => {
            if output.is_json() {
                return Err(CliError::Validation(
                    "`opengoose completion` prints shell scripts directly and does not support --json"
                        .into(),
                ));
            }

            cli::print_completion(shell);
            Ok(())
        }
    }
}
