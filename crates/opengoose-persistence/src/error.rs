use diesel::result::{
    ConnectionError as DieselConnectionError, DatabaseErrorKind, Error as DieselError,
};
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

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("database lock poisoned")]
    LockPoisoned,
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;

impl PersistenceError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Database(diesel::result::Error::NotFound))
    }

    /// Returns `true` when the error is transient and the operation
    /// could succeed on retry (e.g. closed connection, serialization
    /// failure, transient I/O).
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Database(err) => diesel_error_is_transient(err),
            Self::Connection(err) => connection_error_is_transient(err),
            Self::Io(err) => opengoose_types::is_transient_io_error(err),
            _ => false,
        }
    }
}

fn diesel_error_is_transient(err: &DieselError) -> bool {
    match err {
        DieselError::DatabaseError(DatabaseErrorKind::UnableToSendCommand, _)
        | DieselError::DatabaseError(DatabaseErrorKind::SerializationFailure, _)
        | DieselError::DatabaseError(DatabaseErrorKind::ClosedConnection, _)
        | DieselError::RollbackTransaction
        | DieselError::BrokenTransactionManager => true,
        DieselError::RollbackErrorOnCommit {
            rollback_error,
            commit_error,
        } => diesel_error_is_transient(rollback_error) || diesel_error_is_transient(commit_error),
        _ => false,
    }
}

fn connection_error_is_transient(err: &DieselConnectionError) -> bool {
    match err {
        DieselConnectionError::BadConnection(_) => true,
        DieselConnectionError::CouldntSetupConfiguration(err) => diesel_error_is_transient(err),
        DieselConnectionError::InvalidCString(_)
        | DieselConnectionError::InvalidConnectionUrl(_) => false,
        &_ => false,
    }
}

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
    fn test_persistence_error_display_serialization() {
        let err = PersistenceError::Serialization("invalid payload".into());
        assert_eq!(err.to_string(), "serialization error: invalid payload");
    }

    #[test]
    fn test_persistence_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: PersistenceError = io_err.into();
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn test_persistence_error_display_lock_poisoned() {
        let err = PersistenceError::LockPoisoned;
        assert_eq!(err.to_string(), "database lock poisoned");
    }

    #[test]
    fn test_persistence_error_timeout_io_is_transient() {
        let err = PersistenceError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn test_persistence_error_closed_connection_is_transient() {
        let err = PersistenceError::Database(diesel::result::Error::DatabaseError(
            DatabaseErrorKind::ClosedConnection,
            Box::new("closed".to_string()),
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn test_persistence_error_invalid_path_is_not_transient() {
        let err = PersistenceError::InvalidPath;
        assert!(!err.is_transient());
    }
}
