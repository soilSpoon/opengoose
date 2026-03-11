use anyhow::Result;
use clap::Subcommand;

use crate::cmd::output::CliOutput;

mod providers;
mod storage;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose auth list\n  opengoose auth login openai\n  opengoose --json auth models anthropic"
)]
/// Subcommands for `opengoose auth`.
pub enum AuthAction {
    /// Authenticate with an AI provider (supports OAuth and API key)
    #[command(after_help = "Example:\n  opengoose auth login openai")]
    Login {
        /// Provider name (e.g. anthropic, openai). Interactive if omitted.
        provider: Option<String>,
    },
    /// Remove stored credentials for a provider
    #[command(after_help = "Example:\n  opengoose auth logout openai")]
    Logout {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// List all providers and their authentication status
    #[command(after_help = "Examples:\n  opengoose auth list\n  opengoose --json auth ls")]
    #[command(alias = "ls")]
    List,
    /// List available models for a provider
    #[command(
        after_help = "Examples:\n  opengoose auth models openai\n  opengoose --json auth models anthropic"
    )]
    Models {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// Store a custom secret in the OS keyring (e.g. discord_bot_token)
    #[command(after_help = "Example:\n  opengoose auth set discord_bot_token")]
    Set {
        /// Secret key name
        key: String,
    },
    /// Remove a custom secret from the OS keyring
    #[command(after_help = "Example:\n  opengoose auth remove discord_bot_token")]
    Remove {
        /// Secret key name
        key: String,
    },
}

/// Dispatch and execute the selected auth subcommand.
pub async fn execute(action: AuthAction, output: CliOutput) -> Result<()> {
    match action {
        AuthAction::Login { provider } => providers::login(provider.as_deref(), output).await,
        AuthAction::Logout { provider } => storage::logout(&provider, output).await,
        AuthAction::List => providers::list(output).await,
        AuthAction::Models { provider } => providers::models(&provider, output).await,
        AuthAction::Set { key } => storage::set(&key, output),
        AuthAction::Remove { key } => storage::remove(&key, output),
    }
}
