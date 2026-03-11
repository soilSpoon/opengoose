/// Shared error type for YAML-based stores (profiles, teams, etc.).
///
/// This enum consolidates common error variants that appear across multiple
/// store implementations. Crate-specific errors can wrap or extend this type.
#[derive(Debug, thiserror::Error)]
pub enum YamlStoreError {
    /// Resource not found in the store.
    #[error("{resource_type} `{name}` not found")]
    NotFound {
        resource_type: &'static str,
        name: String,
    },

    /// Resource already exists (duplicate).
    #[error("{resource_type} `{name}` already exists (use --force to overwrite)")]
    AlreadyExists {
        resource_type: &'static str,
        name: String,
    },

    /// YAML parsing or serialization error.
    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),

    /// Validation failed (e.g., required field missing).
    #[error("validation failed: {0}")]
    ValidationFailed(String),

    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Could not determine the home directory.
    #[error("could not determine home directory")]
    NoHomeDir,
}

/// Returns `true` when the I/O error kind is typically transient
/// (retryable without code or configuration changes).
///
/// NOTE: `AlreadyExists` is intentionally excluded — it is a deterministic
/// result that retrying will not resolve.
pub fn is_transient_io_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::AddrInUse
            | std::io::ErrorKind::AddrNotAvailable
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NetworkDown
            | std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::UnexpectedEof
    )
}

impl YamlStoreError {
    /// Create a NotFound error for a specific resource type.
    pub fn not_found(resource_type: &'static str, name: impl Into<String>) -> Self {
        Self::NotFound {
            resource_type,
            name: name.into(),
        }
    }

    /// Create an AlreadyExists error for a specific resource type.
    pub fn already_exists(resource_type: &'static str, name: impl Into<String>) -> Self {
        Self::AlreadyExists {
            resource_type,
            name: name.into(),
        }
    }

    /// Returns `true` when the error is transient and the operation could
    /// succeed on retry.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Io(err) => is_transient_io_error(err),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found() {
        let err = YamlStoreError::not_found("profile", "my-agent");
        assert_eq!(err.to_string(), "profile `my-agent` not found");
    }

    #[test]
    fn test_already_exists() {
        let err = YamlStoreError::already_exists("team", "my-team");
        assert_eq!(
            err.to_string(),
            "team `my-team` already exists (use --force to overwrite)"
        );
    }

    #[test]
    fn test_validation_failed() {
        let err = YamlStoreError::ValidationFailed("title is required".into());
        assert_eq!(err.to_string(), "validation failed: title is required");
    }

    #[test]
    fn test_no_home_dir() {
        let err = YamlStoreError::NoHomeDir;
        assert_eq!(err.to_string(), "could not determine home directory");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: YamlStoreError = io_err.into();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_from_yaml_error() {
        let yaml_err = serde_yaml::from_str::<serde_yaml::Value>("invalid: [").unwrap_err();
        let err: YamlStoreError = yaml_err.into();
        assert!(err.to_string().contains("invalid YAML"));
    }

    #[test]
    fn test_is_transient_timeout_io() {
        let err = YamlStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn test_is_transient_not_found_is_false() {
        let err = YamlStoreError::not_found("team", "missing");
        assert!(!err.is_transient());
    }

    #[test]
    fn test_already_exists_is_not_transient() {
        // AlreadyExists is deterministic — retrying will not resolve it.
        let err = YamlStoreError::already_exists("team", "existing-team");
        assert!(!err.is_transient());
    }
}
