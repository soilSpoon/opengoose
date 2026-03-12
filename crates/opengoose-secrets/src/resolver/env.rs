use tracing::debug;

use crate::{SecretKey, SecretValue};

use super::types::{CredentialSource, ResolvedCredential};

/// Try to resolve a secret from the environment variable mapped to `key`.
///
/// Returns `Some(ResolvedCredential)` when the env var is set, `None` otherwise.
pub(crate) fn try_env(env_var: &str, key: &SecretKey) -> Option<ResolvedCredential> {
    std::env::var(env_var).ok().map(|value| {
        debug!(key = key.as_str(), source = "env", env_var = %env_var, "resolved credential");
        ResolvedCredential {
            value: SecretValue::new(value),
            source: CredentialSource::EnvVar,
        }
    })
}
