mod login;
mod register;
mod tokens;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_provider_bridge::{GooseProviderService, ProviderSummary};
use opengoose_secrets::ConfigFile;

/// Subcommands for `opengoose auth`.
#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose auth list\n  opengoose auth login openai\n  opengoose --json auth models anthropic"
)]
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
        AuthAction::Login { provider } => login::cmd_login(provider.as_deref(), output).await,
        AuthAction::Logout { provider } => tokens::cmd_logout(&provider, output).await,
        AuthAction::List => cmd_list(output).await,
        AuthAction::Models { provider } => cmd_models(&provider, output).await,
        AuthAction::Set { key } => tokens::cmd_set(&key, output),
        AuthAction::Remove { key } => tokens::cmd_remove(&key, output),
    }
}

async fn cmd_list(output: CliOutput) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;
    let config = ConfigFile::load()?;

    if output.is_json() {
        let providers_json = providers
            .iter()
            .map(|provider| {
                let auth_type = provider_auth_type(provider);
                let (status, configured_via) = provider_status(provider, &config);
                json!({
                    "name": provider.name,
                    "display_name": provider.display_name,
                    "description": provider.description,
                    "default_model": provider.default_model,
                    "known_models": provider.known_models,
                    "auth": auth_type,
                    "status": status,
                    "configured_via": configured_via,
                })
            })
            .collect::<Vec<_>>();

        output.print_json(&json!({
            "ok": true,
            "command": "auth.list",
            "providers": providers_json,
            "custom_secrets_configured": !config.secrets.is_empty(),
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Providers"));
    let rows = providers
        .iter()
        .map(|provider| {
            let auth_type = provider_auth_type(provider);
            let (status, _configured_via) = provider_status(provider, &config);
            vec![
                provider.display_name.clone(),
                auth_type.to_string(),
                provider.default_model.clone(),
                status.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print!(
        "{}",
        format_table(&["PROVIDER", "AUTH", "DEFAULT MODEL", "STATUS"], &rows)
    );

    if !config.secrets.is_empty() {
        println!();
        println!("Custom secrets: configured");
    }

    Ok(())
}

async fn cmd_models(provider_name: &str, output: CliOutput) -> Result<()> {
    let models = GooseProviderService::fetch_models(provider_name).await?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.models",
            "provider": provider_name,
            "models": models,
        }))?;
        return Ok(());
    }

    if models.is_empty() {
        println!("No models found (provider may not support model listing).");
    } else {
        println!("{}", output.heading(&format!("Models for {provider_name}")));
        let rows = models
            .iter()
            .map(|model| vec![model.clone()])
            .collect::<Vec<_>>();
        print!("{}", format_table(&["MODEL"], &rows));
    }

    Ok(())
}

fn provider_auth_type(provider: &ProviderSummary) -> &'static str {
    let primary_key = provider
        .config_keys
        .iter()
        .find(|key| key.primary)
        .or_else(|| provider.config_keys.first());

    match primary_key {
        Some(key) if key.oauth_flow => "oauth",
        Some(_) => "key",
        None => "none",
    }
}

fn provider_status(
    provider: &ProviderSummary,
    config: &ConfigFile,
) -> (&'static str, Option<&'static str>) {
    let required_keys: Vec<_> = provider
        .config_keys
        .iter()
        .filter(|key| key.required)
        .collect();
    let keyring_keys = config
        .providers
        .get(&provider.name)
        .map(|metadata| &metadata.keys_in_keyring);

    let all_required_in_env = !required_keys.is_empty()
        && required_keys
            .iter()
            .all(|key| std::env::var(&key.name).is_ok_and(|value| !value.is_empty()));
    if all_required_in_env {
        return ("configured", Some("env"));
    }

    let all_required_in_keyring = !required_keys.is_empty()
        && keyring_keys.is_some_and(|keys| {
            required_keys
                .iter()
                .all(|key| keys.contains(&key.name.to_lowercase()))
        });
    if all_required_in_keyring {
        return ("configured", Some("keyring"));
    }

    if required_keys.is_empty() {
        ("ready", None)
    } else {
        ("not configured", None)
    }
}
