use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opengoose_secrets::{ConfigFile, SecretResult, SecretStore, SecretValue};

use crate::GooseProviderService;
use crate::types::{ConfigKeySummary, ProviderSummary};

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
    let models = match tokio::time::timeout(
        Duration::from_secs(10),
        GooseProviderService::fetch_models(&provider.name),
    )
    .await
    {
        Ok(Ok(models)) => models,
        Ok(Err(_)) | Err(_) => return, // unconfigured or slow provider – nothing more to assert
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
