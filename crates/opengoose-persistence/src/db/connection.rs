use std::path::{Path, PathBuf};

use diesel::Connection;
use diesel::sqlite::SqliteConnection;

use crate::error::{PersistenceError, PersistenceResult};

pub(super) fn open_at(path: &Path) -> PersistenceResult<SqliteConnection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let url = path.to_str().ok_or(PersistenceError::InvalidPath)?;
    Ok(SqliteConnection::establish(url)?)
}

pub(super) fn open_in_memory() -> PersistenceResult<SqliteConnection> {
    Ok(SqliteConnection::establish(":memory:")?)
}

pub(super) fn default_path() -> PersistenceResult<PathBuf> {
    let home = dirs::home_dir().ok_or(PersistenceError::NoHomeDir)?;
    Ok(home.join(".opengoose").join("sessions.db"))
}
