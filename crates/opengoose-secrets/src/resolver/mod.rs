mod env;
mod store;
mod types;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::config::ConfigFile;
use crate::keyring_backend::{SecretStore, default_store};
use crate::{SecretError, SecretKey, SecretResult};

pub use types::{CredentialSource, ResolvedCredential};

/// Resolves secrets through: env var -> store -> actionable error.
pub struct CredentialResolver {
    config: ConfigFile,
    store: Arc<dyn SecretStore>,
}

impl CredentialResolver {
    pub fn new() -> SecretResult<Self> {
        let config = ConfigFile::load()?;
        Ok(Self {
            config,
            store: default_store(),
        })
    }

    /// Create a resolver with injected dependencies.
    pub fn with_config_and_store(config: ConfigFile, store: Arc<dyn SecretStore>) -> Self {
        Self { config, store }
    }

    /// Resolve a secret synchronously.
    ///
    /// Resolution order: environment variable -> secret store -> error with guidance.
    pub fn resolve(&self, key: &SecretKey) -> SecretResult<ResolvedCredential> {
        let env_var = self.config.env_var_for(key);

        // 1. Environment variable
        if let Some(cred) = env::try_env(&env_var, key) {
            return Ok(cred);
        }

        // 2. Secret store
        if let Some(cred) = store::try_store(self.store.as_ref(), key)? {
            return Ok(cred);
        }

        // 3. Typed error
        Err(SecretError::NotFound {
            key: key.to_string(),
            env_var,
        })
    }

    /// Async wrapper -- runs the store lookup on a blocking thread since the
    /// keyring crate performs synchronous I/O.
    pub async fn resolve_async(&self, key: &SecretKey) -> SecretResult<ResolvedCredential> {
        let env_var = self.config.env_var_for(key);

        // env var check is cheap, try it first without spawning a thread
        if let Some(cred) = env::try_env(&env_var, key) {
            return Ok(cred);
        }

        // store access needs blocking thread
        if let Some(cred) = store::try_store_async(self.store.clone(), key).await? {
            return Ok(cred);
        }

        Err(SecretError::NotFound {
            key: key.to_string(),
            env_var,
        })
    }
}
