use std::sync::Arc;

use keyring::Entry;
use tracing::debug;

use crate::{SecretResult, SecretValue};

const SERVICE_NAME: &str = "opengoose";

/// Trait for secret storage backends. Enables testing with mock stores.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> SecretResult<Option<SecretValue>>;
    fn set(&self, key: &str, value: &str) -> SecretResult<()>;
    fn delete(&self, key: &str) -> SecretResult<bool>;
}

/// OS keyring backend (macOS Keychain / Linux Secret Service / Windows Credential Manager).
#[derive(Debug, Clone)]
pub struct KeyringBackend;

impl KeyringBackend {
    fn entry(key: &str) -> SecretResult<Entry> {
        Ok(Entry::new(SERVICE_NAME, key)?)
    }
}

/// Real OS keyring implementation — requires system keyring service.
impl SecretStore for KeyringBackend {
    fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(SecretValue::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, key: &str, value: &str) -> SecretResult<()> {
        debug!(key, "storing secret in keyring");
        Self::entry(key)?.set_password(value)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> SecretResult<bool> {
        debug!(key, "deleting secret from keyring");
        match Self::entry(key)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

/// Create the default keyring store.
pub fn default_store() -> Arc<dyn SecretStore> {
    Arc::new(KeyringBackend)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock store for testing.
    pub struct MockStore {
        pub secrets: Mutex<HashMap<String, String>>,
    }

    impl MockStore {
        pub fn new() -> Self {
            Self {
                secrets: Mutex::new(HashMap::new()),
            }
        }

        pub fn with_secret(key: &str, value: &str) -> Self {
            let mut map = HashMap::new();
            map.insert(key.to_owned(), value.to_owned());
            Self {
                secrets: Mutex::new(map),
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

    #[test]
    fn test_mock_store_get_set_delete() {
        let store = MockStore::new();
        assert!(store.get("key").unwrap().is_none());

        store.set("key", "val").unwrap();
        assert_eq!(store.get("key").unwrap().unwrap().as_str(), "val");

        assert!(store.delete("key").unwrap());
        assert!(store.get("key").unwrap().is_none());
        assert!(!store.delete("key").unwrap());
    }

    #[test]
    fn test_mock_store_with_secret() {
        let store = MockStore::with_secret("token", "abc123");
        assert_eq!(store.get("token").unwrap().unwrap().as_str(), "abc123");
    }

    #[test]
    fn test_default_store() {
        let _store = default_store();
        // Just verify construction doesn't panic
    }
}
