use std::fmt;

use anyhow::{bail, Result};
use tracing::debug;

use crate::config::ConfigFile;
use crate::keyring_backend::KeyringBackend;
use crate::{SecretKey, SecretValue};

/// How the credential was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    EnvVar,
    Keyring,
}

impl fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar => f.write_str("environment variable"),
            Self::Keyring => f.write_str("OS keyring"),
        }
    }
}

/// A successfully resolved credential.
pub struct ResolvedCredential {
    pub value: SecretValue,
    pub source: CredentialSource,
}

impl fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("source", &self.source)
            .field("value", &"***")
            .finish()
    }
}

/// Resolves secrets through: env var → keyring → actionable error.
pub struct CredentialResolver {
    config: ConfigFile,
}

impl CredentialResolver {
    pub fn new() -> Result<Self> {
        let config = ConfigFile::load()?;
        Ok(Self { config })
    }

    /// Resolve a secret synchronously.
    ///
    /// Resolution order: environment variable → OS keyring → error with guidance.
    pub fn resolve(&self, key: &SecretKey) -> Result<ResolvedCredential> {
        let env_var = self.config.env_var_for(key);

        // 1. Environment variable
        if let Ok(value) = std::env::var(&env_var) {
            debug!(key = key.as_str(), source = "env", env_var = %env_var, "resolved credential");
            return Ok(ResolvedCredential {
                value: SecretValue::new(value),
                source: CredentialSource::EnvVar,
            });
        }

        // 2. OS keyring
        if let Some(value) = KeyringBackend::get(key.as_str())? {
            debug!(key = key.as_str(), source = "keyring", "resolved credential");
            return Ok(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            });
        }

        // 3. Actionable error
        bail!(
            "Secret `{key}` not found.\n\n\
             To fix this, either:\n  \
             1. Run: opengoose secret set {key}\n  \
             2. Set the environment variable: export {env_var}=<value>"
        );
    }

    /// Async wrapper — runs the sync resolve on a blocking thread since the
    /// `keyring` crate performs synchronous I/O.
    pub async fn resolve_async(&self, key: &SecretKey) -> Result<ResolvedCredential> {
        // env var check is cheap, try it first without spawning a thread
        let env_var = self.config.env_var_for(key);
        if let Ok(value) = std::env::var(&env_var) {
            debug!(key = key.as_str(), source = "env", env_var = %env_var, "resolved credential");
            return Ok(ResolvedCredential {
                value: SecretValue::new(value),
                source: CredentialSource::EnvVar,
            });
        }

        // keyring access needs blocking thread
        let key_str = key.as_str().to_owned();
        let key_display = key.to_string();
        let result = tokio::task::spawn_blocking(move || KeyringBackend::get(&key_str)).await??;

        if let Some(value) = result {
            debug!(key = key.as_str(), source = "keyring", "resolved credential");
            return Ok(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            });
        }

        bail!(
            "Secret `{key_display}` not found.\n\n\
             To fix this, either:\n  \
             1. Run: opengoose secret set {key_display}\n  \
             2. Set the environment variable: export {env_var}=<value>"
        );
    }
}
