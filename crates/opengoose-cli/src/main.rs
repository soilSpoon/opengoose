mod bootstrap;
mod cli;
mod cmd;
mod dispatch;
mod error;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::Parser;

use cli::Cli;
use cmd::output::{CliOutput, OutputMode, print_clap_error, print_error};

// Re-export error types for convenient downstream usage
pub use error::{CliError, CliResult};

fn main() -> ExitCode {
    let requested_json = std::env::args_os().any(|arg| arg == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return print_clap_error(requested_json, err),
    };

    let output = CliOutput::new(OutputMode::from_json_flag(cli.json));
    if let Err(err) = bootstrap::run(cli, output) {
        print_error(output, &err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
