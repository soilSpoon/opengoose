/// Typed errors for the profiles crate.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("profile `{0}` not found")]
    NotFound(String),
    #[error("profile `{0}` already exists (use --force to overwrite)")]
    AlreadyExists(String),
    #[error("skill `{0}` not found")]
    SkillNotFound(String),
    #[error("skill `{0}` already exists (use --force to overwrite)")]
    SkillAlreadyExists(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_error_display_not_found() {
        let err = ProfileError::NotFound("my-agent".into());
        assert_eq!(err.to_string(), "profile `my-agent` not found");
    }

    #[test]
    fn test_profile_error_display_already_exists() {
        let err = ProfileError::AlreadyExists("my-agent".into());
        assert_eq!(
            err.to_string(),
            "profile `my-agent` already exists (use --force to overwrite)"
        );
    }

    #[test]
    fn test_profile_error_display_validation_failed() {
        let err = ProfileError::ValidationFailed("title is required".into());
        assert_eq!(err.to_string(), "validation failed: title is required");
    }

    #[test]
    fn test_profile_error_display_no_home_dir() {
        let err = ProfileError::NoHomeDir;
        assert_eq!(err.to_string(), "could not determine home directory");
    }

    #[test]
    fn test_profile_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: ProfileError = io_err.into();
        assert!(err.to_string().contains("missing"));
    }
}
