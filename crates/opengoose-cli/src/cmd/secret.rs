use anyhow::{bail, Result};
use clap::Subcommand;

use opengoose_secrets::{ConfigFile, KeyringBackend, SecretKey};

#[derive(Subcommand)]
pub enum SecretAction {
    /// Store a secret in the OS keyring
    Set {
        /// Secret key name (e.g. discord_bot_token)
        key: String,
    },
    /// List registered secrets
    List,
    /// Remove a secret from the OS keyring
    Remove {
        /// Secret key name (e.g. discord_bot_token)
        key: String,
    },
}

pub fn execute(action: SecretAction) -> Result<()> {
    match action {
        SecretAction::Set { key } => cmd_set(&key),
        SecretAction::List => cmd_list(),
        SecretAction::Remove { key } => cmd_remove(&key),
    }
}

fn cmd_set(key_name: &str) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let value = rpassword::prompt_password(format!("Enter value for `{key}`: "))?;
    if value.is_empty() {
        bail!("empty value — aborting");
    }

    KeyringBackend::set(key.as_str(), &value)?;

    let mut config = ConfigFile::load()?;
    config.mark_in_keyring(&key);
    config.save()?;

    println!("Stored `{key}` in OS keyring.");
    Ok(())
}

fn cmd_list() -> Result<()> {
    let config = ConfigFile::load()?;

    if config.secrets.is_empty() {
        println!("No secrets registered. Use `opengoose secret set <key>` to add one.");
        return Ok(());
    }

    println!("{:<30} {:<30} {}", "KEY", "ENV VAR", "IN KEYRING");
    println!("{}", "-".repeat(70));
    for (name, meta) in &config.secrets {
        let key = SecretKey::from_str_canonical(name);
        let env_var = match &meta.env_var {
            Some(v) => v.clone(),
            None => key.default_env_var(),
        };
        let in_keyring = if meta.in_keyring { "yes" } else { "no" };
        println!("{:<30} {:<30} {}", name, env_var, in_keyring);
    }
    Ok(())
}

fn cmd_remove(key_name: &str) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let deleted = KeyringBackend::delete(key.as_str())?;

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
