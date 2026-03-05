use std::path::PathBuf;
use std::sync::Mutex;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tracing::info;

use crate::error::{PersistenceError, PersistenceResult};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

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
    pub fn open() -> PersistenceResult<Self> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open or create the database at a specific path.
    pub fn open_at(path: PathBuf) -> PersistenceResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let url = path.to_str().ok_or(PersistenceError::InvalidPath)?;
        let mut conn = SqliteConnection::establish(url)?;
        Self::setup_pragmas(&mut conn)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| PersistenceError::Migration(e.to_string()))?;
        info!(path = %path.display(), "database opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> PersistenceResult<Self> {
        let mut conn = SqliteConnection::establish(":memory:")?;
        Self::setup_pragmas(&mut conn)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| PersistenceError::Migration(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Execute a closure with access to the underlying connection.
    pub(crate) fn with<F, T>(&self, f: F) -> PersistenceResult<T>
    where
        F: FnOnce(&mut SqliteConnection) -> PersistenceResult<T>,
    {
        let mut conn = self.conn.lock().unwrap();
        f(&mut conn)
    }

    fn setup_pragmas(conn: &mut SqliteConnection) -> PersistenceResult<()> {
        diesel::sql_query("PRAGMA journal_mode = WAL").execute(conn)?;
        diesel::sql_query("PRAGMA foreign_keys = ON").execute(conn)?;
        diesel::sql_query("PRAGMA busy_timeout = 5000").execute(conn)?;
        Ok(())
    }

    fn default_path() -> PersistenceResult<PathBuf> {
        let home = dirs::home_dir().ok_or(PersistenceError::NoHomeDir)?;
        Ok(home.join(".opengoose").join("sessions.db"))
    }
}
