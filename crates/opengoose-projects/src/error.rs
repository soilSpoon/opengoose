/// Typed errors for the projects crate.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project `{0}` not found")]
    NotFound(String),

    #[error("project `{0}` already exists (use --force to overwrite)")]
    AlreadyExists(String),

    #[error(transparent)]
    Store(#[from] opengoose_types::YamlStoreError),
}

impl From<std::io::Error> for ProjectError {
    fn from(e: std::io::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::Io(e))
    }
}

impl From<serde_yaml::Error> for ProjectError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::InvalidYaml(e))
    }
}

/// Convenience alias.
pub type ProjectResult<T> = std::result::Result<T, ProjectError>;
