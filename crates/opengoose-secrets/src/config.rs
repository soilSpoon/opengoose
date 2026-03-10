use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{SecretError, SecretKey, SecretResult};

/// On-disk config at `~/.opengoose/config.toml`.
///
/// Stores **metadata only** — never stores secret values.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ConfigFile {
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretMeta>,
    /// Per-provider authentication metadata (e.g. `anthropic`, `openai`).
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderMeta>,
}

/// Per-provider authentication metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProviderMeta {
    /// Which env-var keys are stored in the OS keyring for this provider.
    #[serde(default)]
    pub keys_in_keyring: Vec<String>,
}

/// Per-secret metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    /// Load from an arbitrary path. Returns default if the file does not exist.
    pub fn load_from(path: &Path) -> SecretResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load from `~/.opengoose/config.toml`. Returns default if the file does not exist.
    pub fn load() -> SecretResult<Self> {
        Self::load_from(&Self::path()?)
    }

    /// Save to an arbitrary path, creating the parent directory if needed.
    pub fn save_to(&self, path: &Path) -> SecretResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Save to `~/.opengoose/config.toml`, creating the directory if needed.
    pub fn save(&self) -> SecretResult<()> {
        self.save_to(&Self::path()?)
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
        let entry = self
            .secrets
            .entry(key.as_str().to_owned())
            .or_insert(SecretMeta {
                env_var: None,
                in_keyring: false,
            });
        entry.in_keyring = true;
    }

    /// Remove a key's metadata.
    pub fn remove(&mut self, key: &SecretKey) {
        self.secrets.remove(key.as_str());
    }

    /// Record that a provider's credentials are stored in the keyring.
    /// Merges with any existing keys to preserve credentials from prior runs.
    pub fn mark_provider(&mut self, provider_id: &str, keyring_keys: Vec<String>) {
        let entry = self
            .providers
            .entry(provider_id.to_owned())
            .or_insert_with(|| ProviderMeta {
                keys_in_keyring: Vec::new(),
            });
        for key in keyring_keys {
            if !entry.keys_in_keyring.contains(&key) {
                entry.keys_in_keyring.push(key);
            }
        }
    }

    /// Remove a provider's metadata.
    pub fn remove_provider(&mut self, provider_id: &str) {
        self.providers.remove(provider_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_empty() {
        let config = ConfigFile::default();
        assert!(config.secrets.is_empty());
    }

    #[test]
    fn test_config_env_var_for_default() {
        let config = ConfigFile::default();
        assert_eq!(
            config.env_var_for(&SecretKey::DiscordBotToken),
            "DISCORD_BOT_TOKEN"
        );
    }

    #[test]
    fn test_config_env_var_for_override() {
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "discord_bot_token".into(),
            SecretMeta {
                env_var: Some("MY_CUSTOM_VAR".into()),
                in_keyring: false,
            },
        );
        assert_eq!(
            config.env_var_for(&SecretKey::DiscordBotToken),
            "MY_CUSTOM_VAR"
        );
    }

    #[test]
    fn test_config_mark_in_keyring() {
        let mut config = ConfigFile::default();
        let key = SecretKey::DiscordBotToken;
        config.mark_in_keyring(&key);
        let meta = config.secrets.get(key.as_str()).unwrap();
        assert!(meta.in_keyring);
    }

    #[test]
    fn test_config_remove() {
        let mut config = ConfigFile::default();
        let key = SecretKey::DiscordBotToken;
        config.mark_in_keyring(&key);
        assert!(config.secrets.contains_key(key.as_str()));
        config.remove(&key);
        assert!(!config.secrets.contains_key(key.as_str()));
    }

    #[test]
    fn test_config_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = ConfigFile::default();
        config.mark_in_keyring(&SecretKey::DiscordBotToken);
        config.secrets.insert(
            "custom_key".into(),
            SecretMeta {
                env_var: Some("MY_VAR".into()),
                in_keyring: false,
            },
        );

        config.save_to(&path).unwrap();
        let loaded = ConfigFile::load_from(&path).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn test_config_load_from_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = ConfigFile::load_from(&path).unwrap();
        assert!(config.secrets.is_empty());
    }

    #[test]
    fn test_config_load_from_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "secrets = [\n").unwrap();

        let err = ConfigFile::load_from(&path).unwrap_err();
        assert!(matches!(err, SecretError::ConfigParse(_)));
    }

    #[test]
    fn test_config_mark_in_keyring_updates_existing() {
        let mut config = ConfigFile::default();
        config.secrets.insert(
            "discord_bot_token".into(),
            SecretMeta {
                env_var: Some("CUSTOM_VAR".into()),
                in_keyring: false,
            },
        );
        config.mark_in_keyring(&SecretKey::DiscordBotToken);
        let meta = config.secrets.get("discord_bot_token").unwrap();
        assert!(meta.in_keyring);
        // env_var should be preserved
        assert_eq!(meta.env_var, Some("CUSTOM_VAR".into()));
    }

    #[test]
    fn test_config_mark_provider_merges_keys_and_dedupes() {
        let mut config = ConfigFile::default();
        config.providers.insert(
            "anthropic".into(),
            crate::config::ProviderMeta {
                keys_in_keyring: vec!["A".into(), "B".into()],
            },
        );

        config.mark_provider("anthropic", vec!["B".into(), "C".into(), "C".into()]);

        let provider = config.providers.get("anthropic").unwrap();
        assert_eq!(provider.keys_in_keyring.len(), 3);
        assert!(provider.keys_in_keyring.contains(&"A".to_string()));
        assert!(provider.keys_in_keyring.contains(&"B".to_string()));
        assert!(provider.keys_in_keyring.contains(&"C".to_string()));
    }

    #[test]
    fn test_config_mark_provider_inserts_when_missing() {
        let mut config = ConfigFile::default();
        config.mark_provider("openai", vec!["OPENAI_API_KEY".into()]);

        let provider = config.providers.get("openai").unwrap();
        assert_eq!(provider.keys_in_keyring, vec!["OPENAI_API_KEY"]);
    }

    #[test]
    fn test_config_remove_provider() {
        let mut config = ConfigFile::default();
        config.providers.insert(
            "openai".into(),
            crate::config::ProviderMeta {
                keys_in_keyring: vec!["openai_key".into()],
            },
        );

        config.remove_provider("openai");
        assert!(!config.providers.contains_key("openai"));
    }

    #[test]
    fn test_config_provider_metadata_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("provider-config.toml");
        let mut config = ConfigFile::default();
        config.providers.insert(
            "anthropic".into(),
            crate::config::ProviderMeta {
                keys_in_keyring: vec!["ANTHROPIC_API_KEY".into(), "AZURE_OPENAI_API_KEY".into()],
            },
        );

        config.save_to(&path).expect("save should succeed");
        let loaded = ConfigFile::load_from(&path).unwrap();

        let provider = loaded.providers.get("anthropic").unwrap();
        assert_eq!(
            provider.keys_in_keyring,
            vec!["ANTHROPIC_API_KEY", "AZURE_OPENAI_API_KEY"]
        );
    }

    #[test]
    fn test_config_env_var_for_custom_key() {
        let config = ConfigFile::default();
        assert_eq!(
            config.env_var_for(&SecretKey::Custom("my_api".into())),
            "MY_API"
        );
    }

    #[test]
    fn test_config_save_creates_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subdir").join("config.toml");
        let config = ConfigFile::default();
        config.save_to(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_secret_meta_serialization() {
        let meta = SecretMeta {
            env_var: None,
            in_keyring: true,
        };
        let toml_str = toml::to_string(&meta).unwrap();
        assert!(!toml_str.contains("env_var")); // skip_serializing_if None
        assert!(toml_str.contains("in_keyring = true"));
    }
}
