use std::fmt;
use std::sync::Arc;

use tracing::debug;

use crate::config::ConfigFile;
use crate::keyring_backend::{SecretStore, default_store};
use crate::{SecretError, SecretKey, SecretResult, SecretValue};

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
        if let Ok(value) = std::env::var(&env_var) {
            debug!(key = key.as_str(), source = "env", env_var = %env_var, "resolved credential");
            return Ok(ResolvedCredential {
                value: SecretValue::new(value),
                source: CredentialSource::EnvVar,
            });
        }

        // 2. Secret store
        if let Some(value) = self.store.get(key.as_str())? {
            debug!(
                key = key.as_str(),
                source = "keyring",
                "resolved credential"
            );
            return Ok(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            });
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
        // env var check is cheap, try it first without spawning a thread
        let env_var = self.config.env_var_for(key);
        if let Ok(value) = std::env::var(&env_var) {
            debug!(key = key.as_str(), source = "env", env_var = %env_var, "resolved credential");
            return Ok(ResolvedCredential {
                value: SecretValue::new(value),
                source: CredentialSource::EnvVar,
            });
        }

        // store access needs blocking thread
        let store = self.store.clone();
        let key_str = key.as_str().to_owned();
        let key_display = key.to_string();
        let env_var_clone = env_var.clone();
        let result = tokio::task::spawn_blocking(move || store.get(&key_str)).await??;

        if let Some(value) = result {
            debug!(
                key = key.as_str(),
                source = "keyring",
                "resolved credential"
            );
            return Ok(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            });
        }

        Err(SecretError::NotFound {
            key: key_display,
            env_var: env_var_clone,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyring_backend::tests::MockStore;
    use std::sync::Mutex;

    /// Global lock to serialize tests that mutate process environment variables.
    /// `std::env::set_var`/`remove_var` are unsafe in Rust 2024 because concurrent
    /// env reads/writes are UB. This lock prevents parallel access.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_credential_source_display() {
        assert_eq!(CredentialSource::EnvVar.to_string(), "environment variable");
        assert_eq!(CredentialSource::Keyring.to_string(), "OS keyring");
    }

    #[test]
    fn test_resolved_credential_debug_redacted() {
        let cred = ResolvedCredential {
            value: SecretValue::new("secret123".into()),
            source: CredentialSource::EnvVar,
        };
        let debug = format!("{:?}", cred);
        assert!(debug.contains("***"));
        assert!(!debug.contains("secret123"));
        assert!(debug.contains("EnvVar"));
    }

    #[test]
    fn test_resolve_from_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        let unique_key = "OPENGOOSE_TEST_RESOLVE_ENV_12345";
        unsafe { std::env::set_var(unique_key, "test_token_value") };

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "test_resolve_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: false,
            },
        );

        let resolver =
            CredentialResolver::with_config_and_store(config, Arc::new(MockStore::new()));
        let result = resolver.resolve(&SecretKey::Custom("test_resolve_key".into()));

        unsafe { std::env::remove_var(unique_key) };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "test_token_value");
    }

    #[test]
    fn test_resolve_from_store() {
        let store = Arc::new(MockStore::with_secret("discord_bot_token", "store_token"));

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "discord_bot_token".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_DEFINITELY_NOT_SET_STORE_TEST".into()),
                in_keyring: true,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let cred = resolver.resolve(&SecretKey::DiscordBotToken).unwrap();
        assert_eq!(cred.source, CredentialSource::Keyring);
        assert_eq!(cred.value.as_str(), "store_token");
    }

    #[test]
    fn test_resolve_not_found() {
        let store = Arc::new(MockStore::new());
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "missing_key".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_DEFINITELY_NOT_SET_99999".into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let result = resolver.resolve(&SecretKey::Custom("missing_key".into()));

        match result.unwrap_err() {
            SecretError::NotFound { key, env_var } => {
                assert_eq!(key, "missing_key");
                assert_eq!(env_var, "OPENGOOSE_DEFINITELY_NOT_SET_99999");
            }
            other => panic!("expected NotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_resolve_default_env_var_name() {
        let _guard = ENV_LOCK.lock().unwrap();
        let unique_val = "opengoose_test_discord_token_val";
        unsafe { std::env::set_var("DISCORD_BOT_TOKEN", unique_val) };

        let resolver = CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(MockStore::new()),
        );
        let result = resolver.resolve(&SecretKey::DiscordBotToken);

        unsafe { std::env::remove_var("DISCORD_BOT_TOKEN") };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), unique_val);
    }

    #[tokio::test]
    async fn test_resolve_async_from_env_var() {
        let unique_key = "OPENGOOSE_TEST_ASYNC_RESOLVE_12345";
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var(unique_key, "async_token") };

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "test_async_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: false,
            },
        );
        let resolver =
            CredentialResolver::with_config_and_store(config, Arc::new(MockStore::new()));

        let cred = resolver
            .resolve_async(&SecretKey::Custom("test_async_key".into()))
            .await
            .unwrap();

        unsafe { std::env::remove_var(unique_key) };

        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "async_token");
    }

    #[tokio::test]
    async fn test_resolve_async_from_store() {
        let store = Arc::new(MockStore::with_secret("async_key", "async_store_val"));

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "async_key".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_NOT_SET_ASYNC_STORE".into()),
                in_keyring: true,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let cred = resolver
            .resolve_async(&SecretKey::Custom("async_key".into()))
            .await
            .unwrap();
        assert_eq!(cred.source, CredentialSource::Keyring);
        assert_eq!(cred.value.as_str(), "async_store_val");
    }

    #[tokio::test]
    async fn test_resolve_async_not_found() {
        let store = Arc::new(MockStore::new());
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "gone".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_NOT_SET_GONE".into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let result = resolver
            .resolve_async(&SecretKey::Custom("gone".into()))
            .await;
        assert!(matches!(result.unwrap_err(), SecretError::NotFound { .. }));
    }

    #[test]
    fn test_with_config_and_store() {
        let resolver = CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(MockStore::new()),
        );
        let _ = resolver;
    }
}
