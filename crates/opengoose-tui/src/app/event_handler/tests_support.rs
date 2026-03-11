use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
use opengoose_types::{AppEvent, AppEventKind};

use super::super::state::*;

pub(super) struct MockStore {
    secrets: Mutex<HashMap<String, String>>,
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
            .map(|v| SecretValue::new(v.clone())))
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

pub(super) fn test_app() -> App {
    App::new(AppMode::Normal, None, None)
}

pub(super) fn test_app_with_store(store: Arc<MockStore>) -> App {
    App::with_store(AppMode::Normal, None, None, store, None)
}

pub(super) fn make_event(kind: AppEventKind) -> AppEvent {
    AppEvent {
        kind,
        timestamp: Instant::now(),
    }
}
