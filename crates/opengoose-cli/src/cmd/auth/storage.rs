use crate::error::{CliError, CliResult};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_provider_bridge::GooseProviderService;
use opengoose_secrets::{ConfigFile, KeyringBackend, SecretKey, SecretStore};

pub(super) async fn logout(provider_id: &str, output: CliOutput) -> CliResult<()> {
    let providers = GooseProviderService::list_providers().await;
    let mut config = ConfigFile::load()?;

    let mut keys_to_delete = std::collections::BTreeSet::new();
    if let Some(metadata) = config.providers.get(provider_id) {
        for key in &metadata.keys_in_keyring {
            keys_to_delete.insert(key.clone());
        }
    }
    if let Some(provider) = providers
        .iter()
        .find(|provider| provider.name == provider_id)
    {
        for key in &provider.config_keys {
            keys_to_delete.insert(key.name.to_lowercase());
        }
    }
    if keys_to_delete.is_empty() && !config.providers.contains_key(provider_id) {
        return Err(CliError::Validation(format!(
            "unknown provider `{provider_id}` and no stored credentials found"
        )));
    }

    let mut errors = Vec::new();
    for keyring_key in &keys_to_delete {
        if let Err(err) = KeyringBackend.delete(keyring_key) {
            errors.push(format!("{keyring_key}: {err}"));
        }
    }

    if !errors.is_empty() {
        let display = providers
            .iter()
            .find(|provider| provider.name == provider_id)
            .map(|provider| provider.display_name.as_str())
            .unwrap_or(provider_id);
        return Err(CliError::Validation(format!(
            "failed to remove some credentials for {display}: {}",
            errors.join("; ")
        )));
    }

    config.remove_provider(provider_id);
    config.save()?;

    let display = providers
        .iter()
        .find(|provider| provider.name == provider_id)
        .map(|provider| provider.display_name.as_str())
        .unwrap_or(provider_id);

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.logout",
            "provider": provider_id,
            "display_name": display,
            "removed_keys": keys_to_delete,
        }))?;
    } else {
        println!("Logged out from {display}.");
    }

    Ok(())
}

pub(super) fn set(key_name: &str, output: CliOutput) -> CliResult<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let value = rpassword::prompt_password(format!("Enter value for `{key}`: "))?;
    if value.is_empty() {
        return Err(CliError::Validation(format!("empty value — aborting")));
    }

    KeyringBackend.set(key.as_str(), &value)?;

    let mut config = ConfigFile::load()?;
    config.mark_in_keyring(&key);
    config.save()?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.set",
            "key": key.as_str(),
            "stored": true,
        }))?;
    } else {
        println!("Stored `{key}` in OS keyring.");
    }

    Ok(())
}

pub(super) fn remove(key_name: &str, output: CliOutput) -> CliResult<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let deleted = KeyringBackend.delete(key.as_str())?;

    let mut config = ConfigFile::load()?;
    config.remove(&key);
    config.save()?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.remove",
            "key": key.as_str(),
            "removed": deleted,
        }))?;
    } else if deleted {
        println!("Removed `{key}` from OS keyring.");
    } else {
        println!("`{key}` was not in the OS keyring (metadata cleared).");
    }

    Ok(())
}
