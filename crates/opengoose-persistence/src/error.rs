use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("no home directory found")]
    NoHomeDir,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid enum value: {0}")]
    InvalidEnumValue(String),
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;
