use std::fmt;

use tracing::debug;

use crate::config::ConfigFile;
use crate::keyring_backend::KeyringBackend;
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

/// Resolves secrets through: env var -> keyring -> actionable error.
pub struct CredentialResolver {
    config: ConfigFile,
}

impl CredentialResolver {
    pub fn new() -> SecretResult<Self> {
        let config = ConfigFile::load()?;
        Ok(Self { config })
    }

    /// Create a resolver with a provided config (useful for testing).
    pub fn with_config(config: ConfigFile) -> Self {
        Self { config }
    }

    /// Resolve a secret synchronously.
    ///
    /// Resolution order: environment variable -> OS keyring -> error with guidance.
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

        // 2. OS keyring
        if let Some(value) = KeyringBackend::get(key.as_str())? {
            debug!(key = key.as_str(), source = "keyring", "resolved credential");
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

    /// Async wrapper -- runs the sync resolve on a blocking thread since the
    /// `keyring` crate performs synchronous I/O.
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

        // keyring access needs blocking thread
        let key_str = key.as_str().to_owned();
        let key_display = key.to_string();
        let env_var_clone = env_var.clone();
        let result = tokio::task::spawn_blocking(move || KeyringBackend::get(&key_str)).await??;

        if let Some(value) = result {
            debug!(key = key.as_str(), source = "keyring", "resolved credential");
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
        let unique_key = "OPENGOOSE_TEST_RESOLVE_ENV_12345";
        unsafe { std::env::set_var(unique_key, "test_token_value") };

        let mut config = ConfigFile::default();
        // Override the env var for a custom key to our unique one
        config.secrets.insert(
            "test_resolve_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config(config);
        let result = resolver.resolve(&SecretKey::Custom("test_resolve_key".into()));

        unsafe { std::env::remove_var(unique_key) };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "test_token_value");
    }

    #[test]
    fn test_resolve_not_found_or_keyring_error() {
        // Use a unique env var name that definitely doesn't exist
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "nonexistent_key".into(),
            crate::config::SecretMeta {
                env_var: Some("OPENGOOSE_DEFINITELY_NOT_SET_99999".into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config(config);
        let result = resolver.resolve(&SecretKey::Custom("nonexistent_key".into()));

        // Should error — either NotFound (if keyring works) or KeyringError (if no D-Bus)
        assert!(result.is_err());
        match result.unwrap_err() {
            SecretError::NotFound { key, env_var } => {
                assert_eq!(key, "nonexistent_key");
                assert_eq!(env_var, "OPENGOOSE_DEFINITELY_NOT_SET_99999");
            }
            SecretError::KeyringError(_) => {
                // Expected in environments without D-Bus session
            }
            other => panic!("expected NotFound or KeyringError, got: {:?}", other),
        }
    }

    #[test]
    fn test_resolve_default_env_var_name() {
        let unique_val = "opengoose_test_discord_token_val";
        unsafe { std::env::set_var("DISCORD_BOT_TOKEN", unique_val) };

        let resolver = CredentialResolver::with_config(ConfigFile::default());
        let result = resolver.resolve(&SecretKey::DiscordBotToken);

        unsafe { std::env::remove_var("DISCORD_BOT_TOKEN") };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), unique_val);
    }

    #[tokio::test]
    async fn test_resolve_async_from_env_var() {
        let unique_key = "OPENGOOSE_TEST_ASYNC_RESOLVE_12345";
        unsafe { std::env::set_var(unique_key, "async_token") };

        let mut config = ConfigFile::default();
        config.secrets.insert(
            "test_async_key".into(),
            crate::config::SecretMeta {
                env_var: Some(unique_key.into()),
                in_keyring: false,
            },
        );

        let resolver = CredentialResolver::with_config(config);
        let result = resolver
            .resolve_async(&SecretKey::Custom("test_async_key".into()))
            .await;

        unsafe { std::env::remove_var(unique_key) };

        let cred = result.unwrap();
        assert_eq!(cred.source, CredentialSource::EnvVar);
        assert_eq!(cred.value.as_str(), "async_token");
    }

    #[test]
    fn test_with_config() {
        let config = ConfigFile::default();
        let resolver = CredentialResolver::with_config(config);
        // Should not panic - just verifying construction works
        let _ = resolver;
    }
}
