mod config;
mod keyring_backend;
mod resolver;

pub use config::ConfigFile;
pub use keyring_backend::KeyringBackend;
pub use resolver::{CredentialResolver, CredentialSource, ResolvedCredential};

use std::fmt;

use zeroize::Zeroize;

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
