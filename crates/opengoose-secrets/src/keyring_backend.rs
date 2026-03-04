use anyhow::{Context, Result};
use keyring::Entry;
use tracing::debug;

use crate::SecretValue;

const SERVICE_NAME: &str = "opengoose";

/// Thin wrapper around the OS keyring (macOS Keychain / Linux Secret Service / Windows Credential Manager).
pub struct KeyringBackend;

impl KeyringBackend {
    fn entry(key: &str) -> Result<Entry> {
        Entry::new(SERVICE_NAME, key).context("failed to create keyring entry")
    }

    /// Store a secret in the OS keyring.
    pub fn set(key: &str, value: &str) -> Result<()> {
        debug!(key, "storing secret in keyring");
        Self::entry(key)?
            .set_password(value)
            .context("failed to store secret in keyring")
    }

    /// Retrieve a secret from the OS keyring. Returns `None` if the entry does not exist.
    pub fn get(key: &str) -> Result<Option<SecretValue>> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(SecretValue::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e).context("failed to read secret from keyring")),
        }
    }

    /// Delete a secret from the OS keyring. Returns `false` if the entry did not exist.
    pub fn delete(key: &str) -> Result<bool> {
        debug!(key, "deleting secret from keyring");
        match Self::entry(key)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(anyhow::anyhow!(e).context("failed to delete secret from keyring")),
        }
    }
}
