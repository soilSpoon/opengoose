use std::fmt;

use crate::SecretValue;

/// How the credential was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    EnvVar,
    Keyring,
}

impl fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar => f.write_str("environment variable"),
            Self::Keyring => f.write_str("OS keyring"),
        }
    }
}

/// A successfully resolved credential.
pub struct ResolvedCredential {
    pub value: SecretValue,
    pub source: CredentialSource,
}

impl fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("source", &self.source)
            .field("value", &"***")
            .finish()
    }
}
