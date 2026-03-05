/// Typed errors for the teams crate.
#[derive(Debug, thiserror::Error)]
pub enum TeamError {
    #[error("team `{0}` not found")]
    NotFound(String),
    #[error("team `{0}` already exists (use --force to overwrite)")]
    AlreadyExists(String),
    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("profile `{0}` not found")]
    ProfileNotFound(String),
    #[error("agent failed: {0}")]
    AgentFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not determine home directory")]
    NoHomeDir,
}

/// Convenience alias.
pub type TeamResult<T> = std::result::Result<T, TeamError>;
