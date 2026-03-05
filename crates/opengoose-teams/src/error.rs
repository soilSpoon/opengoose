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
    fn test_team_error_display_validation_failed() {
        let err = TeamError::ValidationFailed("title is required".into());
        assert_eq!(err.to_string(), "validation failed: title is required");
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
    fn test_team_error_display_no_home_dir() {
        let err = TeamError::NoHomeDir;
        assert_eq!(err.to_string(), "could not determine home directory");
    }

    #[test]
    fn test_team_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: TeamError = io_err.into();
        assert!(err.to_string().contains("file missing"));
    }
}
