use std::io::Write;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_provider_bridge::{ConfigKeySummary, GooseProviderService, ProviderSummary};
use opengoose_secrets::{ConfigFile, KeyringBackend, SecretKey, SecretStore};

#[derive(Subcommand)]
pub enum AuthAction {
    /// Authenticate with an AI provider (supports OAuth and API key)
    Login {
        /// Provider name (e.g. anthropic, openai). Interactive if omitted.
        provider: Option<String>,
    },
    /// Remove stored credentials for a provider
    Logout {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// List all providers and their authentication status
    #[command(alias = "ls")]
    List,
    /// List available models for a provider
    Models {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// Store a custom secret in the OS keyring (e.g. discord_bot_token)
    Set {
        /// Secret key name
        key: String,
    },
    /// Remove a custom secret from the OS keyring
    Remove {
        /// Secret key name
        key: String,
    },
}

pub async fn execute(action: AuthAction) -> Result<()> {
    match action {
        AuthAction::Login { provider } => cmd_login(provider.as_deref()).await,
        AuthAction::Logout { provider } => cmd_logout(&provider).await,
        AuthAction::List => cmd_list().await,
        AuthAction::Models { provider } => cmd_models(&provider).await,
        AuthAction::Set { key } => cmd_set(&key),
        AuthAction::Remove { key } => cmd_remove(&key),
    }
}

async fn cmd_login(provider_arg: Option<&str>) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;

    let provider = match provider_arg {
        Some(id) => match providers.iter().find(|p| p.name == id) {
            Some(p) => p,
            None => {
                eprintln!("Unknown provider: {id}");
                eprintln!();
                print_available_providers(&providers);
                bail!("unknown provider `{id}`");
            }
        },
        None => prompt_provider_selection(&providers)?,
    };

    if provider.config_keys.is_empty() {
        println!(
            "{} does not require authentication. Just set it as your provider.",
            provider.display_name
        );
        return Ok(());
    }

    let has_oauth = provider.config_keys.iter().any(|k| k.oauth_flow);
    let non_oauth_count = provider
        .config_keys
        .iter()
        .filter(|k| !k.oauth_flow)
        .count();

    if has_oauth {
        println!(
            "Configuring {} (OAuth + credentials)",
            provider.display_name
        );
    } else {
        println!(
            "Configuring {} ({non_oauth_count} credential{} needed)",
            provider.display_name,
            if non_oauth_count != 1 { "s" } else { "" }
        );
    }

    // Handle OAuth keys first
    for key in provider.config_keys.iter().filter(|k| k.oauth_flow) {
        println!(
            "Starting OAuth authentication for {} ({})...",
            provider.display_name, key.name
        );
        GooseProviderService::run_oauth(&provider.name).await?;
        println!("OAuth authentication completed.");
    }

    // Collect all manual credential inputs before storing
    let mut collected: Vec<(String, String)> = Vec::new();
    for key in provider.config_keys.iter().filter(|k| !k.oauth_flow) {
        if !key.required {
            eprint!("  Configure {} (optional)? [y/N]: ", key.name);
            std::io::stderr().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                continue;
            }
        }

        let value = if key.secret {
            rpassword::prompt_password(format!("  {} [{}]: ", key_label(key), key.name))?
        } else {
            prompt_text_input(key)?
        };

        if value.is_empty() {
            if key.required {
                bail!("empty value for {} — aborting", key.name);
            }
            continue;
        }

        collected.push((key.name.clone(), value));
    }

    // Store all credentials only after successful collection
    for (env_var, value) in &collected {
        GooseProviderService::store_credential(&provider.name, env_var, value)?;
    }

    println!("Authenticated with {}.", provider.display_name);

    // Show available models after authentication
    match GooseProviderService::fetch_models(&provider.name).await {
        Ok(models) if !models.is_empty() => {
            println!("\nAvailable models ({}):", models.len());
            for (i, model) in models.iter().take(10).enumerate() {
                println!("  {:>2}. {}", i + 1, model);
            }
            if models.len() > 10 {
                println!(
                    "  ... and {} more (use `opengoose auth models {}` to see all)",
                    models.len() - 10,
                    provider.name
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::debug!("Could not fetch models after login: {e}");
        }
    }

    Ok(())
}

async fn cmd_logout(provider_id: &str) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;
    let mut config = ConfigFile::load()?;

    // Delete from keyring using config metadata
    let mut errors = Vec::new();
    if let Some(meta) = config.providers.get(provider_id) {
        for keyring_key in &meta.keys_in_keyring {
            if let Err(e) = KeyringBackend.delete(keyring_key) {
                errors.push(format!("{keyring_key}: {e}"));
            }
        }
    } else if let Some(provider) = providers.iter().find(|p| p.name == provider_id) {
        for key in &provider.config_keys {
            if let Err(e) = KeyringBackend.delete(&key.name.to_lowercase()) {
                errors.push(format!("{}: {e}", key.name));
            }
        }
    } else {
        bail!("unknown provider `{provider_id}` and no stored credentials found");
    }

    if !errors.is_empty() {
        let display = providers
            .iter()
            .find(|p| p.name == provider_id)
            .map(|p| p.display_name.as_str())
            .unwrap_or(provider_id);
        bail!(
            "failed to remove some credentials for {display}: {}",
            errors.join("; ")
        );
    }

    config.remove_provider(provider_id);
    config.save()?;

    let display = providers
        .iter()
        .find(|p| p.name == provider_id)
        .map(|p| p.display_name.as_str())
        .unwrap_or(provider_id);
    println!("Logged out from {display}.");
    Ok(())
}

async fn cmd_list() -> Result<()> {
    let providers = GooseProviderService::list_providers().await;
    let config = ConfigFile::load()?;

    println!("{:<22} {:<8} STATUS", "PROVIDER", "AUTH");
    println!("{}", "-".repeat(50));

    for provider in &providers {
        if provider.config_keys.is_empty() {
            println!("{:<22} {:<8} ready", provider.display_name, "none");
            continue;
        }

        let primary_key = provider
            .config_keys
            .iter()
            .find(|k| k.primary)
            .or_else(|| provider.config_keys.first());

        let auth_type = match primary_key {
            Some(k) if k.oauth_flow => "oauth",
            Some(_) => "key",
            None => "—",
        };

        let required_keys: Vec<_> = provider.config_keys.iter().filter(|k| k.required).collect();
        let keyring_keys = config
            .providers
            .get(&provider.name)
            .map(|m| &m.keys_in_keyring);

        let all_required_in_env = !required_keys.is_empty()
            && required_keys.iter().all(|k| std::env::var(&k.name).is_ok());
        let all_required_in_keyring = !required_keys.is_empty()
            && keyring_keys.is_some_and(|keys| {
                required_keys
                    .iter()
                    .all(|k| keys.contains(&k.name.to_lowercase()))
            });

        let status = if all_required_in_env {
            "configured (env)"
        } else if all_required_in_keyring {
            "configured (keyring)"
        } else if required_keys.is_empty() {
            // No required keys — provider is usable
            "ready"
        } else {
            "not configured"
        };

        println!("{:<22} {:<8} {status}", provider.display_name, auth_type);
    }

    // Also show custom secrets
    if !config.secrets.is_empty() {
        println!();
        println!("Custom secrets:");
        for (name, meta) in &config.secrets {
            let source = if meta.in_keyring { "keyring" } else { "env" };
            println!("  {:<28} {source}", name);
        }
    }

    Ok(())
}

async fn cmd_models(provider_name: &str) -> Result<()> {
    eprintln!("Fetching models for {provider_name}...");
    let models = GooseProviderService::fetch_models(provider_name).await?;

    if models.is_empty() {
        println!("No models found (provider may not support model listing).");
    } else {
        println!("Available models ({}):", models.len());
        for model in &models {
            println!("  {model}");
        }
    }

    Ok(())
}

fn cmd_set(key_name: &str) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let value = rpassword::prompt_password(format!("Enter value for `{key}`: "))?;
    if value.is_empty() {
        bail!("empty value — aborting");
    }

    KeyringBackend.set(key.as_str(), &value)?;

    let mut config = ConfigFile::load()?;
    config.mark_in_keyring(&key);
    config.save()?;

    println!("Stored `{key}` in OS keyring.");
    Ok(())
}

fn cmd_remove(key_name: &str) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let deleted = KeyringBackend.delete(key.as_str())?;

    let mut config = ConfigFile::load()?;
    config.remove(&key);
    config.save()?;

    if deleted {
        println!("Removed `{key}` from OS keyring.");
    } else {
        println!("`{key}` was not in the OS keyring (metadata cleared).");
    }
    Ok(())
}

fn prompt_provider_selection(providers: &[ProviderSummary]) -> Result<&ProviderSummary> {
    let items: Vec<_> = providers
        .iter()
        .filter(|p| !p.config_keys.is_empty())
        .collect();

    eprintln!("Select a provider:");
    for (i, p) in items.iter().enumerate() {
        let auth_hint = if p.config_keys.iter().any(|k| k.oauth_flow) {
            " (OAuth)"
        } else {
            ""
        };
        eprintln!("  [{:>2}] {}{auth_hint}", i + 1, p.display_name);
    }
    eprintln!();
    eprint!("Enter number: ");
    std::io::stderr().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    let idx: usize = input
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid selection"))?;

    items
        .get(idx.wrapping_sub(1))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("selection out of range (enter 1–{})", items.len()))
}

fn print_available_providers(providers: &[ProviderSummary]) {
    eprintln!("Available providers:");
    for p in providers {
        let auth = if p.config_keys.iter().any(|k| k.oauth_flow) {
            "oauth"
        } else if p.config_keys.is_empty() {
            "none"
        } else {
            "key"
        };
        eprintln!("  {:<20} {:<20} auth: {auth}", p.name, p.display_name);
    }
}

fn key_label(key: &ConfigKeySummary) -> &str {
    if key.name.ends_with("_API_KEY") || key.name.ends_with("_KEY") {
        "API Key"
    } else if key.name.ends_with("_TOKEN") {
        "Token"
    } else if key.name.contains("HOST") || key.name.contains("ENDPOINT") {
        "URL"
    } else if key.name.contains("REGION") {
        "Region"
    } else if key.name.contains("PROFILE") {
        "Profile"
    } else if key.name.contains("PROJECT") {
        "Project ID"
    } else if key.name.contains("LOCATION") {
        "Location"
    } else if key.name.contains("DEPLOYMENT") {
        "Deployment"
    } else {
        "Value"
    }
}

fn prompt_text_input(key: &ConfigKeySummary) -> Result<String> {
    let label = key_label(key);
    let prompt = match &key.default {
        Some(d) => format!("  {label} [{} (default: {d})]: ", key.name),
        None => format!("  {label} [{}]: ", key.name),
    };
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty()
        && let Some(d) = &key.default
    {
        return Ok(d.clone());
    }
    Ok(trimmed)
}
