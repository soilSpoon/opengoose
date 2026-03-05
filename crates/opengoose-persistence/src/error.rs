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
