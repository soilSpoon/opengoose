use opengoose_provider_bridge::{ConfigKeySummary, ProviderSummary};
use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::app::state::{App, AppMode};

pub(super) struct MockStore {
    pub(super) secrets: Mutex<HashMap<String, String>>,
}

impl MockStore {
    pub(super) fn new() -> Self {
        Self {
            secrets: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretStore for MockStore {
    fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
        Ok(self
            .secrets
            .lock()
            .unwrap()
            .get(key)
            .map(|value| SecretValue::new(value.clone())))
    }

    fn set(&self, key: &str, value: &str) -> SecretResult<()> {
        self.secrets
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> SecretResult<bool> {
        Ok(self.secrets.lock().unwrap().remove(key).is_some())
    }
}

pub(super) fn test_app_with_store() -> (App, Arc<MockStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let store = Arc::new(MockStore::new());
    let app = App::with_store(
        AppMode::Normal,
        None,
        None,
        store.clone(),
        Some(config_path),
    );
    (app, store, dir)
}

pub(super) fn make_provider(
    name: &str,
    display: &str,
    keys: Vec<ConfigKeySummary>,
) -> ProviderSummary {
    ProviderSummary {
        name: name.into(),
        display_name: display.into(),
        description: "desc".into(),
        default_model: "model".into(),
        known_models: vec![],
        config_keys: keys,
    }
}

pub(super) fn api_key(name: &str) -> ConfigKeySummary {
    ConfigKeySummary {
        name: name.into(),
        required: true,
        secret: true,
        oauth_flow: false,
        default: None,
        primary: true,
    }
}
