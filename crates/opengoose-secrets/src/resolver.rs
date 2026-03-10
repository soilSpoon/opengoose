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

    struct FailingStore;

    impl SecretStore for FailingStore {
        fn get(&self, _key: &str) -> SecretResult<Option<SecretValue>> {
            Err(SecretError::ConfigIo(std::io::Error::new(
                std::io::ErrorKind::Other,
                "credential backend unavailable",
            )))
        }

        fn set(&self, _key: &str, _value: &str) -> SecretResult<()> {
            Ok(())
        }

        fn delete(&self, _key: &str) -> SecretResult<bool> {
            Ok(false)
        }
    }

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
    fn test_resolve_propagates_store_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("DISCORD_BOT_TOKEN") };

        let resolver = CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(FailingStore),
        );
        let err = resolver.resolve(&SecretKey::DiscordBotToken).unwrap_err();
        assert!(matches!(err, SecretError::ConfigIo(_)));
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
            other => unreachable!("expected NotFound, got: {:?}", other),
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
    #[allow(clippy::await_holding_lock)] // Single-threaded test; lock must span await to prevent env var races
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
    #[allow(clippy::await_holding_lock)]
    async fn test_resolve_async_propagates_store_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let resolver = CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(FailingStore),
        );
        let err = resolver
            .resolve_async(&SecretKey::DiscordBotToken)
            .await
            .unwrap_err();
        assert!(matches!(err, SecretError::ConfigIo(_)));
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

    /// Env var takes precedence over the keyring when both provide the same key.
    #[test]
    fn test_env_var_takes_precedence_over_keyring() {
        let _guard = ENV_LOCK.lock().unwrap();
        let unique_key = "OPENGOOSE_TEST_PRECEDENCE_99887";
        unsafe { std::env::set_var(unique_key, "env_value") };

        let store = Arc::new(MockStore::with_secret("precedence_key", "store_value"));
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "precedence_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: true,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let cred = resolver
            .resolve(&SecretKey::Custom("precedence_key".into()))
            .unwrap();

        unsafe { std::env::remove_var(unique_key) };

        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "env_value");
    }

    /// Provider registry integration: resolve each key in a multi-key provider.
    #[test]
    fn test_provider_registry_all_keys_missing() {
        use crate::provider_registry::find_provider;

        let provider = find_provider("azure").unwrap();
        let store = Arc::new(MockStore::new());
        let resolver = CredentialResolver::with_config_and_store(ConfigFile::default(), store);

        // None of the Azure env vars are set in CI; all should return NotFound.
        for key_info in provider.keys {
            // Use a clean env (any real env vars with these names would cause false negatives,
            // so we only assert on the error case when the env var is absent).
            if std::env::var(key_info.env_var).is_err() {
                let secret_key = SecretKey::Custom(key_info.env_var.to_lowercase());
                let result = resolver.resolve(&SecretKey::Custom(key_info.env_var.to_lowercase()));
                match result {
                    Err(SecretError::NotFound { .. }) => {}
                    Ok(_) => {
                        // env var happened to be set in the test environment — skip
                        let _ = secret_key;
                    }
                    Err(other) => panic!("unexpected error: {:?}", other),
                }
            }
        }
    }

    /// Provider registry integration: resolve using provider env var name.
    #[test]
    fn test_provider_registry_resolve_from_env() {
        use crate::provider_registry::find_provider;

        let _guard = ENV_LOCK.lock().unwrap();
        let provider = find_provider("anthropic").unwrap();
        let key_info = &provider.keys[0];
        let test_value = "test_anthropic_key_xyz_99";

        unsafe { std::env::set_var(key_info.env_var, test_value) };

        let resolver = CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(MockStore::new()),
        );
        let result = resolver.resolve(&SecretKey::Custom(key_info.env_var.to_lowercase()));

        unsafe { std::env::remove_var(key_info.env_var) };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), test_value);
    }

    /// Secret values and credentials must not appear in error output.
    #[test]
    fn test_secret_not_in_error_message() {
        let store = Arc::new(MockStore::new());
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "sensitive_key".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_DEFINITELY_NOT_SET_SENSITIVE".into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let err = resolver
            .resolve(&SecretKey::Custom("sensitive_key".into()))
            .unwrap_err();

        let err_msg = err.to_string();
        // Error message should reference the key/env var for actionability, not a secret value.
        assert!(
            err_msg.contains("sensitive_key")
                || err_msg.contains("OPENGOOSE_DEFINITELY_NOT_SET_SENSITIVE")
        );
        // Confirm SecretValue Debug stays redacted.
        let val = crate::SecretValue::new("hunter2".into());
        let debug = format!("{:?}", val);
        assert!(!debug.contains("hunter2"));
    }

    /// Resolver handles multiple independent secret keys correctly.
    #[test]
    fn test_multiple_secrets_same_resolver() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Set env var for first key.
        unsafe { std::env::set_var("OPENGOOSE_TEST_MULTI_A_77771", "val_a") };

        // Store holds second key.
        let store = Arc::new(MockStore::with_secret("multi_b", "val_b"));

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "multi_a".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_TEST_MULTI_A_77771".into()),
                in_keyring: false,
            },
        );
        config.secrets.insert(
            "multi_b".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_TEST_MULTI_B_77771".into()),
                in_keyring: true,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);

        let cred_a = resolver
            .resolve(&SecretKey::Custom("multi_a".into()))
            .unwrap();
        let cred_b = resolver
            .resolve(&SecretKey::Custom("multi_b".into()))
            .unwrap();

        unsafe { std::env::remove_var("OPENGOOSE_TEST_MULTI_A_77771") };

        assert_eq!(cred_a.source, CredentialSource::EnvVar);
        assert_eq!(cred_a.value.as_str(), "val_a");
        assert_eq!(cred_b.source, CredentialSource::Keyring);
        assert_eq!(cred_b.value.as_str(), "val_b");
    }

    /// Resolving one key does not contaminate resolution of another.
    #[test]
    fn test_no_cross_contamination_between_keys() {
        let store = Arc::new(MockStore::with_secret("key_one", "secret_one"));
        let resolver = CredentialResolver::with_config_and_store(ConfigFile::default(), store);

        let cred_one = resolver
            .resolve(&SecretKey::Custom("key_one".into()))
            .unwrap();
        let err_two = resolver
            .resolve(&SecretKey::Custom("key_two".into()))
            .unwrap_err();

        assert_eq!(cred_one.value.as_str(), "secret_one");
        assert!(matches!(err_two, SecretError::NotFound { .. }));
    }

    /// Async: env var wins over keyring when both are present.
    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_resolve_async_env_takes_precedence_over_store() {
        let _guard = ENV_LOCK.lock().unwrap();
        let unique_key = "OPENGOOSE_TEST_ASYNC_PREC_44321";
        unsafe { std::env::set_var(unique_key, "async_env_val") };

        let store = Arc::new(MockStore::with_secret("async_prec_key", "async_store_val"));
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "async_prec_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: true,
            },
        );

        let resolver = CredentialResolver::with_config_and_store(config, store);
        let cred = resolver
            .resolve_async(&SecretKey::Custom("async_prec_key".into()))
            .await
            .unwrap();

        unsafe { std::env::remove_var(unique_key) };

        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "async_env_val");
    }

    /// All well-known SecretKey variants return NotFound when nothing is configured.
    #[test]
    fn test_all_well_known_keys_return_not_found_when_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Temporarily unset the default env vars for the well-known keys.
        let well_known = [
            (SecretKey::DiscordBotToken, "DISCORD_BOT_TOKEN"),
            (SecretKey::TelegramBotToken, "TELEGRAM_BOT_TOKEN"),
            (SecretKey::SlackBotToken, "SLACK_BOT_TOKEN"),
            (SecretKey::SlackAppToken, "SLACK_APP_TOKEN"),
            (SecretKey::MatrixHomeserverUrl, "MATRIX_HOMESERVER_URL"),
            (SecretKey::MatrixAccessToken, "MATRIX_ACCESS_TOKEN"),
        ];

        // Save original values and clear them.
        let originals: Vec<Option<String>> = well_known
            .iter()
            .map(|(_, env)| std::env::var(env).ok())
            .collect();
        for (_, env) in &well_known {
            unsafe { std::env::remove_var(env) };
        }

        let store = Arc::new(MockStore::new());
        let resolver = CredentialResolver::with_config_and_store(ConfigFile::default(), store);

        for (key, _) in &well_known {
            let result = resolver.resolve(key);
            assert!(
                matches!(result, Err(SecretError::NotFound { .. })),
                "expected NotFound for {:?}",
                key
            );
        }

        // Restore original values.
        for ((_, env), original) in well_known.iter().zip(originals.iter()) {
            if let Some(val) = original {
                unsafe { std::env::set_var(env, val) };
            }
        }
    }
}
