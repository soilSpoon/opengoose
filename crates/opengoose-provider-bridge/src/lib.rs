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
        let provider =
            goose::providers::create(provider_name, model_config, vec![]).await?;

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
        let provider =
            goose::providers::create(provider_name, model_config, vec![]).await?;

        provider
            .configure_oauth()
            .await
            .map_err(|e| anyhow::anyhow!("OAuth failed for {}: {}", provider_name, e))
    }

    /// Store a credential value in the OS keyring and update config metadata.
    pub fn store_credential(
        provider_id: &str,
        env_var: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        let keyring_key = env_var.to_lowercase();
        KeyringBackend.set(&keyring_key, value)?;

        let mut config = ConfigFile::load()?;
        // Merge with existing keys_in_keyring
        let entry = config
            .providers
            .entry(provider_id.to_string())
            .or_default();
        if !entry.keys_in_keyring.contains(&keyring_key) {
            entry.keys_in_keyring.push(keyring_key);
        }
        config.save()?;
        Ok(())
    }
}
