mod config;
mod keyring_backend;
mod resolver;

pub use config::ConfigFile;
pub use keyring_backend::KeyringBackend;
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
    Custom(String),
}

impl SecretKey {
    /// Canonical string key used for keyring account name and config section.
    pub fn as_str(&self) -> &str {
        match self {
            Self::DiscordBotToken => "discord_bot_token",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Default environment variable name for this key.
    pub fn default_env_var(&self) -> String {
        match self {
            Self::DiscordBotToken => "DISCORD_BOT_TOKEN".into(),
            Self::Custom(s) => s.to_uppercase(),
        }
    }

    /// Parse from canonical string.
    pub fn from_str_canonical(s: &str) -> Self {
        match s {
            "discord_bot_token" => Self::DiscordBotToken,
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
