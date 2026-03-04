use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{SecretError, SecretKey, SecretResult};

/// On-disk config at `~/.opengoose/config.toml`.
///
/// Stores **metadata only** — never stores secret values.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretMeta>,
}

/// Per-secret metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMeta {
    /// Override the default environment variable name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
    /// Whether this secret was stored via `opengoose secret set`.
    #[serde(default)]
    pub in_keyring: bool,
}

impl ConfigFile {
    fn path() -> SecretResult<PathBuf> {
        let home = dirs::home_dir().ok_or(SecretError::NoHomeDir)?;
        Ok(home.join(".opengoose").join("config.toml"))
    }

    /// Load from `~/.opengoose/config.toml`. Returns default if the file does not exist.
    pub fn load() -> SecretResult<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save to `~/.opengoose/config.toml`, creating the directory if needed.
    pub fn save(&self) -> SecretResult<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Get the environment variable name to check for a given key.
    pub fn env_var_for(&self, key: &SecretKey) -> String {
        self.secrets
            .get(key.as_str())
            .and_then(|m| m.env_var.clone())
            .unwrap_or_else(|| key.default_env_var())
    }

    /// Mark a key as stored in the keyring.
    pub fn mark_in_keyring(&mut self, key: &SecretKey) {
        let entry = self.secrets.entry(key.as_str().to_owned()).or_insert(SecretMeta {
            env_var: None,
            in_keyring: false,
        });
        entry.in_keyring = true;
    }

    /// Remove a key's metadata.
    pub fn remove(&mut self, key: &SecretKey) {
        self.secrets.remove(key.as_str());
    }
}
