use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("no home directory found")]
    NoHomeDir,

    #[error("invalid database path (non-UTF-8)")]
    InvalidPath,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid enum value: {0}")]
    InvalidEnumValue(String),
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistence_error_display_migration() {
        let err = PersistenceError::Migration("bad migration".into());
        assert_eq!(err.to_string(), "migration error: bad migration");
    }

    #[test]
    fn test_persistence_error_display_no_home_dir() {
        let err = PersistenceError::NoHomeDir;
        assert_eq!(err.to_string(), "no home directory found");
    }

    #[test]
    fn test_persistence_error_display_invalid_path() {
        let err = PersistenceError::InvalidPath;
        assert_eq!(err.to_string(), "invalid database path (non-UTF-8)");
    }

    #[test]
    fn test_persistence_error_display_invalid_enum_value() {
        let err = PersistenceError::InvalidEnumValue("unknown RunStatus: bogus".into());
        assert_eq!(
            err.to_string(),
            "invalid enum value: unknown RunStatus: bogus"
        );
    }

    #[test]
    fn test_persistence_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: PersistenceError = io_err.into();
        assert!(err.to_string().contains("access denied"));
    }
}
