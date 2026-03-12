mod connection;
mod migrations;
mod pragmas;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Mutex;

use diesel::sqlite::SqliteConnection;
use tracing::{info, instrument};

use crate::error::{PersistenceError, PersistenceResult};

/// SQL literal for `datetime('now')` — shared across all stores.
pub(crate) fn now_sql() -> diesel::expression::SqlLiteral<diesel::sql_types::Text> {
    diesel::dsl::sql::<diesel::sql_types::Text>("datetime('now')")
}

/// Nullable variant of `now_sql()` for optional timestamp columns.
pub(crate) fn now_sql_nullable()
-> diesel::expression::SqlLiteral<diesel::sql_types::Nullable<diesel::sql_types::Text>> {
    diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::Text>>("datetime('now')")
}

/// Shared SQLite database wrapper.
///
/// All persistence modules (sessions, message queue, work items, orchestration runs)
/// share the same `Arc<Database>` to operate on a single connection.
pub struct Database {
    conn: Mutex<SqliteConnection>,
}

impl Database {
    /// Open or create the database at `~/.opengoose/sessions.db`.
    #[instrument]
    pub fn open() -> PersistenceResult<Self> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open or create the database at a specific path.
    #[instrument(fields(path = %path.display()))]
    pub fn open_at(path: PathBuf) -> PersistenceResult<Self> {
        let mut conn = connection::open_at(&path)?;
        Self::prepare_connection(&mut conn)?;
        info!(path = %path.display(), "database opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (for testing).
    #[instrument]
    pub fn open_in_memory() -> PersistenceResult<Self> {
        let mut conn = connection::open_in_memory()?;
        Self::prepare_connection(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Execute a closure with access to the underlying connection.
    #[instrument(skip(self, f))]
    pub(crate) fn with<F, T>(&self, f: F) -> PersistenceResult<T>
    where
        F: FnOnce(&mut SqliteConnection) -> PersistenceResult<T>,
    {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
        f(&mut conn)
    }

    fn prepare_connection(conn: &mut SqliteConnection) -> PersistenceResult<()> {
        pragmas::configure(conn)?;
        migrations::run_pending(conn)?;
        Ok(())
    }

    fn default_path() -> PersistenceResult<PathBuf> {
        connection::default_path()
    }
}
