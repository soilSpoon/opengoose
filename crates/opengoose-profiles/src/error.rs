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
    #[error(transparent)]
    Store(#[from] opengoose_types::YamlStoreError),
}

impl From<std::io::Error> for ProfileError {
    fn from(e: std::io::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::Io(e))
    }
}

impl From<serde_yaml::Error> for ProfileError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Store(opengoose_types::YamlStoreError::InvalidYaml(e))
    }
}

/// Convenience alias.
pub type ProfileResult<T> = std::result::Result<T, ProfileError>;

impl ProfileError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Store(err) => err.is_transient(),
            _ => false,
        }
    }
}

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
    fn test_profile_error_display_skill_not_found() {
        let err = ProfileError::SkillNotFound("my-skill".into());
        assert_eq!(err.to_string(), "skill `my-skill` not found");
    }

    #[test]
    fn test_profile_error_display_skill_already_exists() {
        let err = ProfileError::SkillAlreadyExists("my-skill".into());
        assert_eq!(
            err.to_string(),
            "skill `my-skill` already exists (use --force to overwrite)"
        );
    }

    #[test]
    fn test_profile_error_from_store_error() {
        let store_err =
            opengoose_types::YamlStoreError::ValidationFailed("title is required".into());
        let err: ProfileError = store_err.into();
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn test_profile_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let store_err = opengoose_types::YamlStoreError::Io(io_err);
        let err: ProfileError = store_err.into();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_profile_error_transient_when_store_io_is_transient() {
        let err = ProfileError::Store(opengoose_types::YamlStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        )));
        assert!(err.is_transient());
    }
}
