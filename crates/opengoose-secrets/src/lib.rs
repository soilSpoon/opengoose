//! Secret and credential management for OpenGoose.
//!
//! Provides a layered resolution strategy for API keys and secrets:
//! 1. Environment variables (highest priority).
//! 2. OS keyring via [`KeyringBackend`].
//! 3. An encrypted config file ([`ConfigFile`]).
//!
//! Key types: [`CredentialResolver`], [`SecretStore`], [`ProviderInfo`].

mod config;
mod keyring_backend;
mod provider_registry;
mod resolver;

pub use config::{ConfigFile, ProviderMeta};
pub use keyring_backend::{KeyringBackend, SecretStore, default_store};
pub use provider_registry::{KeyInfo, ProviderInfo, all_providers, find_provider};
pub use resolver::{CredentialResolver, CredentialSource, ResolvedCredential};

use std::fmt;

use zeroize::Zeroize;

/// Typed errors for the secrets crate.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret `{key}` not found (env: {env_var})")]
    NotFound { key: String, env_var: String },
    #[error("keyring access failed: {0}")]
    KeyringError(#[from] keyring::Error),
    #[error("config I/O error: {0}")]
    ConfigIo(#[from] std::io::Error),
    #[error("config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("config serialize error: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),
    #[error("could not determine home directory")]
    NoHomeDir,
    #[error("async task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

/// Convenience alias used throughout the secrets crate.
pub type SecretResult<T> = std::result::Result<T, SecretError>;

/// Well-known secret identifiers with extensibility via `Custom`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretKey {
    DiscordBotToken,
    TelegramBotToken,
    SlackBotToken,
    SlackAppToken,
    MatrixHomeserverUrl,
    MatrixAccessToken,
    Custom(String),
}

impl SecretKey {
    /// Canonical string key used for keyring account name and config section.
    pub fn as_str(&self) -> &str {
        match self {
            Self::DiscordBotToken => "discord_bot_token",
            Self::TelegramBotToken => "telegram_bot_token",
            Self::SlackBotToken => "slack_bot_token",
            Self::SlackAppToken => "slack_app_token",
            Self::MatrixHomeserverUrl => "matrix_homeserver_url",
            Self::MatrixAccessToken => "matrix_access_token",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Default environment variable name for this key.
    pub fn default_env_var(&self) -> String {
        match self {
            Self::DiscordBotToken => "DISCORD_BOT_TOKEN".into(),
            Self::TelegramBotToken => "TELEGRAM_BOT_TOKEN".into(),
            Self::SlackBotToken => "SLACK_BOT_TOKEN".into(),
            Self::SlackAppToken => "SLACK_APP_TOKEN".into(),
            Self::MatrixHomeserverUrl => "MATRIX_HOMESERVER_URL".into(),
            Self::MatrixAccessToken => "MATRIX_ACCESS_TOKEN".into(),
            Self::Custom(s) => s.to_uppercase(),
        }
    }

    /// Parse from canonical string.
    pub fn from_str_canonical(s: &str) -> Self {
        match s {
            "discord_bot_token" => Self::DiscordBotToken,
            "telegram_bot_token" => Self::TelegramBotToken,
            "slack_bot_token" => Self::SlackBotToken,
            "slack_app_token" => Self::SlackAppToken,
            "matrix_homeserver_url" => Self::MatrixHomeserverUrl,
            "matrix_access_token" => Self::MatrixAccessToken,
            other => Self::Custom(other.to_owned()),
        }
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A secret value that is zeroed on drop.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_key_as_str_discord() {
        assert_eq!(SecretKey::DiscordBotToken.as_str(), "discord_bot_token");
    }

    #[test]
    fn test_secret_key_as_str_custom() {
        assert_eq!(SecretKey::Custom("my_key".into()).as_str(), "my_key");
    }

    #[test]
    fn test_secret_key_default_env_var_discord() {
        assert_eq!(
            SecretKey::DiscordBotToken.default_env_var(),
            "DISCORD_BOT_TOKEN"
        );
    }

    #[test]
    fn test_secret_key_default_env_var_custom() {
        assert_eq!(
            SecretKey::Custom("my_api_key".into()).default_env_var(),
            "MY_API_KEY"
        );
    }

    #[test]
    fn test_secret_key_from_str_canonical_known() {
        assert_eq!(
            SecretKey::from_str_canonical("discord_bot_token"),
            SecretKey::DiscordBotToken
        );
    }

    #[test]
    fn test_secret_key_from_str_canonical_unknown() {
        assert_eq!(
            SecretKey::from_str_canonical("other"),
            SecretKey::Custom("other".into())
        );
    }

    #[test]
    fn test_secret_value_debug_redacted() {
        let val = SecretValue::new("hunter2".into());
        let debug = format!("{:?}", val);
        assert!(debug.contains("***"));
        assert!(!debug.contains("hunter2"));
    }

    #[test]
    fn test_secret_value_as_str() {
        let val = SecretValue::new("abc".into());
        assert_eq!(val.as_str(), "abc");
    }

    #[test]
    fn test_secret_key_display() {
        assert_eq!(SecretKey::DiscordBotToken.to_string(), "discord_bot_token");
        assert_eq!(SecretKey::Custom("my_thing".into()).to_string(), "my_thing");
    }

    #[test]
    fn test_secret_key_eq() {
        assert_eq!(SecretKey::DiscordBotToken, SecretKey::DiscordBotToken);
        assert_ne!(
            SecretKey::DiscordBotToken,
            SecretKey::Custom("discord_bot_token".into())
        );
        assert_eq!(SecretKey::Custom("a".into()), SecretKey::Custom("a".into()));
    }

    #[test]
    fn test_secret_error_display() {
        let err = SecretError::NotFound {
            key: "test".into(),
            env_var: "TEST_VAR".into(),
        };
        assert_eq!(err.to_string(), "secret `test` not found (env: TEST_VAR)");

        let err = SecretError::NoHomeDir;
        assert_eq!(err.to_string(), "could not determine home directory");
    }

    #[test]
    fn test_secret_value_new_empty() {
        let val = SecretValue::new(String::new());
        assert_eq!(val.as_str(), "");
    }
}
