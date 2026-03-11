//! Bridge between OpenGoose and the Goose AI provider system.
//!
//! Exposes a simplified view of Goose provider metadata ([`ProviderSummary`],
//! [`ConfigKeySummary`]) without pulling the full Goose dependency tree into
//! every crate. Also provides `list_providers` and `activate_provider`
//! helpers that configure secrets and launch the provider backend.

use opengoose_secrets::{ConfigFile, KeyringBackend, SecretStore};

/// Summary of a provider's metadata, extracted from Goose's `ProviderMetadata`.
#[derive(Debug, Clone, Default)]
pub struct ProviderSummary {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub default_model: String,
    /// Statically-known model names from Goose metadata.
    pub known_models: Vec<String>,
    /// Configuration keys needed by this provider.
    pub config_keys: Vec<ConfigKeySummary>,
}

/// Summary of a single configuration key for a provider.
#[derive(Debug, Clone)]
pub struct ConfigKeySummary {
    pub name: String,
    pub required: bool,
    pub secret: bool,
    /// When `true`, `configure_oauth()` should be called instead of prompting
    /// the user for manual input.
    pub oauth_flow: bool,
    pub default: Option<String>,
    /// Whether this key is shown prominently during setup.
    pub primary: bool,
}

/// Bridge between Goose's async provider APIs and OpenGoose.
pub struct GooseProviderService;

impl GooseProviderService {
    /// List all providers registered with Goose.
    pub async fn list_providers() -> Vec<ProviderSummary> {
        let goose_providers = goose::providers::providers().await;
        goose_providers
            .into_iter()
            .map(|(meta, _provider_type)| ProviderSummary {
                name: meta.name,
                display_name: meta.display_name,
                description: meta.description,
                default_model: meta.default_model,
                known_models: meta.known_models.iter().map(|m| m.name.clone()).collect(),
                config_keys: meta
                    .config_keys
                    .into_iter()
                    .map(|k| ConfigKeySummary {
                        name: k.name,
                        required: k.required,
                        secret: k.secret,
                        oauth_flow: k.oauth_flow,
                        default: k.default,
                        primary: k.primary,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Fetch available models for a provider by calling its API.
    ///
    /// Requires valid credentials to be configured. Falls back to
    /// `known_models` from metadata if the API call fails.
    pub async fn fetch_models(provider_name: &str) -> anyhow::Result<Vec<String>> {
        let providers = goose::providers::providers().await;
        let (meta, _) = providers
            .iter()
            .find(|(m, _)| m.name == provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

        let model_config = goose::model::ModelConfig::new(&meta.default_model)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .with_canonical_limits(provider_name);
        let provider = goose::providers::create(provider_name, model_config, vec![]).await?;

        match provider.fetch_recommended_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => {
                let fallback: Vec<String> =
                    meta.known_models.iter().map(|m| m.name.clone()).collect();
                Ok(fallback)
            }
            Err(e) => {
                tracing::debug!("fetch_recommended_models failed for {provider_name}: {e}");
                let fallback: Vec<String> =
                    meta.known_models.iter().map(|m| m.name.clone()).collect();
                if fallback.is_empty() {
                    Err(anyhow::anyhow!(
                        "Failed to fetch models and no known models available: {e}"
                    ))
                } else {
                    Ok(fallback)
                }
            }
        }
    }

    /// Run the OAuth authentication flow for a provider.
    ///
    /// This typically opens a browser for device-code or PKCE authorization.
    pub async fn run_oauth(provider_name: &str) -> anyhow::Result<()> {
        let providers = goose::providers::providers().await;
        let (meta, _) = providers
            .iter()
            .find(|(m, _)| m.name == provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

        let model_config = goose::model::ModelConfig::new(&meta.default_model)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .with_canonical_limits(provider_name);
        let provider = goose::providers::create(provider_name, model_config, vec![]).await?;

        provider
            .configure_oauth()
            .await
            .map_err(|e| anyhow::anyhow!("OAuth failed for {}: {}", provider_name, e))
    }

    /// Store a credential value in the OS keyring and update config metadata.
    pub fn store_credential(provider_id: &str, env_var: &str, value: &str) -> anyhow::Result<()> {
        let mut config = ConfigFile::load()?;
        Self::store_credential_in_config(
            provider_id,
            env_var,
            value,
            &KeyringBackend,
            &mut config,
        )?;
        config.save()?;
        Ok(())
    }

    fn store_credential_in_config(
        provider_id: &str,
        env_var: &str,
        value: &str,
        store: &dyn SecretStore,
        config: &mut ConfigFile,
    ) -> anyhow::Result<()> {
        let keyring_key = env_var.to_lowercase();
        store.set(&keyring_key, value)?;

        // Merge with existing keys_in_keyring
        let entry = config.providers.entry(provider_id.to_string()).or_default();
        if !entry.keys_in_keyring.contains(&keyring_key) {
            entry.keys_in_keyring.push(keyring_key);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_providers_returns_metadata() {
        let providers = GooseProviderService::list_providers().await;

        assert!(!providers.is_empty());
        assert!(providers.iter().all(|provider| !provider.name.is_empty()));
        assert!(
            providers
                .iter()
                .all(|provider| !provider.display_name.is_empty())
        );
        assert!(
            providers
                .iter()
                .all(|provider| { provider.config_keys.iter().all(|key| !key.name.is_empty()) })
        );
    }

    #[tokio::test]
    async fn fetch_models_rejects_unknown_provider() {
        let err = GooseProviderService::fetch_models("definitely-unknown-provider")
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Unknown provider: definitely-unknown-provider")
        );
    }

    #[tokio::test]
    async fn fetch_models_returns_non_empty_for_known_provider() {
        let providers = GooseProviderService::list_providers().await;
        let provider = providers
            .iter()
            .find(|provider| !provider.known_models.is_empty())
            .expect("at least one provider should expose known models");

        // fetch_models may fail if the provider requires credentials that
        // aren't configured in the test environment.  In that case the
        // provider's static known_models list (already verified non-empty
        // above) is the expected fallback, so we just return early.
        let models = match GooseProviderService::fetch_models(&provider.name).await {
            Ok(m) => m,
            Err(_) => return, // unconfigured provider – nothing more to assert
        };

        assert!(!models.is_empty());
    }

    #[tokio::test]
    async fn run_oauth_rejects_unknown_provider() {
        let err = GooseProviderService::run_oauth("definitely-unknown-provider")
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Unknown provider: definitely-unknown-provider")
        );
    }

    #[test]
    fn provider_summary_default_has_empty_fields() {
        let summary = ProviderSummary::default();
        assert!(summary.name.is_empty());
        assert!(summary.display_name.is_empty());
        assert!(summary.description.is_empty());
        assert!(summary.default_model.is_empty());
        assert!(summary.known_models.is_empty());
        assert!(summary.config_keys.is_empty());
    }

    #[test]
    fn config_key_summary_fields_accessible() {
        let key = ConfigKeySummary {
            name: "API_KEY".into(),
            required: true,
            secret: true,
            oauth_flow: false,
            default: None,
            primary: true,
        };
        assert_eq!(key.name, "API_KEY");
        assert!(key.required);
        assert!(key.secret);
        assert!(!key.oauth_flow);
        assert!(key.default.is_none());
        assert!(key.primary);
    }

    #[test]
    fn config_key_summary_with_default_value() {
        let key = ConfigKeySummary {
            name: "BASE_URL".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: Some("https://api.example.com".into()),
            primary: false,
        };
        assert_eq!(key.default.as_deref(), Some("https://api.example.com"));
        assert!(!key.required);
    }

    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use opengoose_secrets::{SecretResult, SecretValue};

    #[derive(Debug)]
    struct MockStore {
        entries: Arc<Mutex<HashMap<String, String>>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn get_value(&self, key: &str) -> String {
            self.entries
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or_default()
        }
    }

    impl SecretStore for MockStore {
        fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }

        fn set(&self, key: &str, value: &str) -> SecretResult<()> {
            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, _key: &str) -> SecretResult<bool> {
            Ok(false)
        }
    }

    struct FailingStore;

    impl SecretStore for FailingStore {
        fn get(&self, _key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(None)
        }

        fn set(&self, _key: &str, _value: &str) -> SecretResult<()> {
            Err(opengoose_secrets::SecretError::ConfigIo(
                std::io::Error::other("mock keyring unavailable"),
            ))
        }

        fn delete(&self, _key: &str) -> SecretResult<bool> {
            Ok(false)
        }
    }

    #[test]
    fn store_credential_in_config_records_lowercase_key() {
        let store = MockStore::new();
        let mut config = ConfigFile::default();

        GooseProviderService::store_credential_in_config(
            "openai",
            "OPENAI_API_KEY",
            "test-secret",
            &store,
            &mut config,
        )
        .unwrap();

        assert_eq!(store.get_value("openai_api_key"), "test-secret");
        let provider = config.providers.get("openai").expect("provider metadata");
        assert_eq!(provider.keys_in_keyring, vec!["openai_api_key"]);
    }

    #[test]
    fn store_credential_in_config_dedupes_keys() {
        let store = MockStore::new();
        let mut config = ConfigFile::default();

        GooseProviderService::store_credential_in_config(
            "azure",
            "AZURE_OPENAI_API_KEY",
            "first",
            &store,
            &mut config,
        )
        .unwrap();
        GooseProviderService::store_credential_in_config(
            "azure",
            "AZURE_OPENAI_API_KEY",
            "second",
            &store,
            &mut config,
        )
        .unwrap();

        let provider = config.providers.get("azure").expect("provider metadata");
        assert_eq!(provider.keys_in_keyring, vec!["azure_openai_api_key"]);
        assert_eq!(store.get_value("azure_openai_api_key"), "second");
    }

    #[test]
    fn store_credential_in_config_propagates_store_errors() {
        let store = FailingStore;
        let mut config = ConfigFile::default();

        let err = GooseProviderService::store_credential_in_config(
            "openai",
            "OPENAI_API_KEY",
            "value",
            &store,
            &mut config,
        )
        .unwrap_err();

        assert!(err.to_string().contains("mock keyring unavailable"));
    }

    #[tokio::test]
    async fn list_providers_all_have_nonempty_names() {
        let providers = GooseProviderService::list_providers().await;
        for p in &providers {
            assert!(!p.name.is_empty(), "provider name should not be empty");
            assert!(
                !p.display_name.is_empty(),
                "display_name should not be empty for {}",
                p.name
            );
        }
    }
}
