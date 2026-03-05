/// Typed errors for the profiles crate.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("profile `{0}` not found")]
    NotFound(String),
    #[error("profile `{0}` already exists (use --force to overwrite)")]
    AlreadyExists(String),
    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not determine home directory")]
    NoHomeDir,
}

/// Convenience alias.
pub type ProfileResult<T> = std::result::Result<T, ProfileError>;
