/// Typed errors for the teams crate.
#[derive(Debug, thiserror::Error)]
pub enum TeamError {
    #[error("team `{0}` not found")]
    NotFound(String),
    #[error("team `{0}` already exists (use --force to overwrite)")]
    AlreadyExists(String),
    #[error("profile `{0}` not found")]
    ProfileNotFound(String),
    #[error("agent failed: {0}")]
    AgentFailed(String),
    #[error("persistence error: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),
    #[error(transparent)]
    Store(#[from] opengoose_types::YamlStoreError),
}

impl From<std::io::Error> for TeamError {
    fn from(e: std::io::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::Io(e))
    }
}

impl From<serde_yaml::Error> for TeamError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::InvalidYaml(e))
    }
}

/// Convenience alias.
pub type TeamResult<T> = std::result::Result<T, TeamError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_error_display_not_found() {
        let err = TeamError::NotFound("my-team".into());
        assert_eq!(err.to_string(), "team `my-team` not found");
    }

    #[test]
    fn test_team_error_display_already_exists() {
        let err = TeamError::AlreadyExists("my-team".into());
        assert_eq!(
            err.to_string(),
            "team `my-team` already exists (use --force to overwrite)"
        );
    }

    #[test]
    fn test_team_error_display_profile_not_found() {
        let err = TeamError::ProfileNotFound("coder".into());
        assert_eq!(err.to_string(), "profile `coder` not found");
    }

    #[test]
    fn test_team_error_display_agent_failed() {
        let err = TeamError::AgentFailed("timeout".into());
        assert_eq!(err.to_string(), "agent failed: timeout");
    }

    #[test]
    fn test_team_error_from_store_error() {
        let store_err =
            opengoose_types::YamlStoreError::ValidationFailed("title is required".into());
        let err: TeamError = store_err.into();
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn test_team_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let store_err = opengoose_types::YamlStoreError::Io(io_err);
        let err: TeamError = store_err.into();
        assert!(err.to_string().contains("file missing"));
    }
}
