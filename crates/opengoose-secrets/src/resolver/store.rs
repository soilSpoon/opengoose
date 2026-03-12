use std::sync::Arc;

use tracing::debug;

use crate::keyring_backend::SecretStore;
use crate::{SecretKey, SecretResult};

use super::types::{CredentialSource, ResolvedCredential};

/// Try to resolve a secret from the backing store (keyring).
///
/// Returns `Ok(Some(..))` when the store holds the key, `Ok(None)` when absent,
/// and propagates store errors.
pub(crate) fn try_store(
    store: &dyn SecretStore,
    key: &SecretKey,
) -> SecretResult<Option<ResolvedCredential>> {
    match store.get(key.as_str())? {
        Some(value) => {
            debug!(
                key = key.as_str(),
                source = "keyring",
                "resolved credential"
            );
            Ok(Some(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            }))
        }
        None => Ok(None),
    }
}

/// Async variant — runs the store lookup on a blocking thread since the keyring
/// crate performs synchronous I/O.
pub(crate) async fn try_store_async(
    store: Arc<dyn SecretStore>,
    key: &SecretKey,
) -> SecretResult<Option<ResolvedCredential>> {
    let key_str = key.as_str().to_owned();
    let key_ref = key.as_str().to_owned();
    let result = tokio::task::spawn_blocking(move || store.get(&key_str)).await??;

    match result {
        Some(value) => {
            debug!(key = %key_ref, source = "keyring", "resolved credential");
            Ok(Some(ResolvedCredential {
                value,
                source: CredentialSource::Keyring,
            }))
        }
        None => Ok(None),
    }
}
