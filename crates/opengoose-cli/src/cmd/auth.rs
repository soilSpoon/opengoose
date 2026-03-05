use std::io::Write;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_secrets::{
    ConfigFile, KeyringBackend, SecretKey, SecretStore, all_providers, find_provider,
};

#[derive(Subcommand)]
pub enum AuthAction {
    /// Authenticate with an AI provider (interactive selection if provider omitted)
    Login {
        /// Provider name (e.g. anthropic, openai, google)
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

pub fn execute(action: AuthAction) -> Result<()> {
    match action {
        AuthAction::Login { provider } => cmd_login(provider.as_deref()),
        AuthAction::Logout { provider } => cmd_logout(&provider),
        AuthAction::List => cmd_list(),
        AuthAction::Set { key } => cmd_set(&key),
        AuthAction::Remove { key } => cmd_remove(&key),
    }
}

fn cmd_login(provider_arg: Option<&str>) -> Result<()> {
    let provider = match provider_arg {
        Some(id) => match find_provider(id) {
            Some(p) => p,
            None => {
                eprintln!("Unknown provider: {id}");
                eprintln!();
                print_available_providers();
                bail!("unknown provider `{id}`");
            }
        },
        None => prompt_provider_selection()?,
    };

    if provider.no_auth_required() {
        println!(
            "{} does not require authentication. Just set it as your provider.",
            provider.display_name
        );
        return Ok(());
    }

    println!(
        "Configuring {} ({} credential{} needed)",
        provider.display_name,
        provider.keys.len(),
        if provider.keys.len() > 1 { "s" } else { "" }
    );

    let mut keyring_keys = Vec::new();

    for key_info in provider.keys {
        let value = if key_info.secret {
            rpassword::prompt_password(format!("  {} [{}]: ", key_info.label, key_info.env_var))?
        } else {
            eprint!("  {} [{}]: ", key_info.label, key_info.env_var);
            std::io::stderr().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        };

        if value.is_empty() {
            bail!("empty value for {} — aborting", key_info.env_var);
        }

        let keyring_key = key_info.env_var.to_lowercase();
        KeyringBackend.set(&keyring_key, &value)?;
        keyring_keys.push(keyring_key);
    }

    let mut config = ConfigFile::load()?;
    config.mark_provider(provider.id, keyring_keys);
    config.save()?;

    println!("Authenticated with {}.", provider.display_name);
    Ok(())
}

fn cmd_logout(provider_id: &str) -> Result<()> {
    let mut config = ConfigFile::load()?;

    // Delete from keyring using config metadata
    if let Some(meta) = config.providers.get(provider_id) {
        for keyring_key in &meta.keys_in_keyring {
            let _ = KeyringBackend.delete(keyring_key);
        }
    } else if let Some(provider) = find_provider(provider_id) {
        // Fallback: try deleting known keys even without config entry
        for key_info in provider.keys {
            let _ = KeyringBackend.delete(&key_info.env_var.to_lowercase());
        }
    } else {
        bail!(
            "unknown provider `{provider_id}` and no stored credentials found"
        );
    }

    config.remove_provider(provider_id);
    config.save()?;

    let display = find_provider(provider_id)
        .map(|p| p.display_name)
        .unwrap_or(provider_id);
    println!("Logged out from {display}.");
    Ok(())
}

fn cmd_list() -> Result<()> {
    let config = ConfigFile::load()?;

    println!(
        "{:<20} {:<25} {}",
        "PROVIDER", "ENV VAR", "STATUS"
    );
    println!("{}", "-".repeat(65));

    for provider in all_providers() {
        if provider.no_auth_required() {
            println!(
                "{:<20} {:<25} {}",
                provider.display_name, "—", "no auth needed"
            );
            continue;
        }

        let primary_env = provider.keys[0].env_var;

        // Check env var
        let env_set = provider
            .keys
            .iter()
            .any(|k| std::env::var(k.env_var).is_ok());

        // Check keyring
        let in_keyring = config
            .providers
            .get(provider.id)
            .is_some_and(|m| !m.keys_in_keyring.is_empty());

        let status = if env_set {
            "configured (env)"
        } else if in_keyring {
            "configured (keyring)"
        } else {
            "not configured"
        };

        println!("{:<20} {:<25} {status}", provider.display_name, primary_env);
    }

    // Also show custom secrets
    if !config.secrets.is_empty() {
        println!();
        println!("Custom secrets:");
        for (name, meta) in &config.secrets {
            let key = SecretKey::from_str_canonical(name);
            let env_var = match &meta.env_var {
                Some(v) => v.clone(),
                None => key.default_env_var(),
            };
            let source = if meta.in_keyring { "keyring" } else { "env" };
            println!("  {:<28} {:<25} {source}", name, env_var);
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

fn prompt_provider_selection() -> Result<&'static opengoose_secrets::ProviderInfo> {
    let providers: Vec<_> = all_providers()
        .iter()
        .filter(|p| !p.no_auth_required())
        .collect();

    eprintln!("Select a provider:");
    for (i, p) in providers.iter().enumerate() {
        let key_count = p.keys.len();
        let suffix = if key_count > 1 {
            format!(" ({key_count} keys)")
        } else {
            String::new()
        };
        eprintln!("  [{:>2}] {}{}", i + 1, p.display_name, suffix);
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

    providers
        .get(idx - 1)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("selection out of range"))
}

fn print_available_providers() {
    eprintln!("Available providers:");
    for p in all_providers() {
        eprintln!("  {:<20} {}", p.id, p.display_name);
    }
}
