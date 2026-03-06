use std::path::PathBuf;
use std::sync::Mutex;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
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
        // In WAL mode, NORMAL is safe and significantly faster than the FULL default.
        // WAL provides atomicity; the only risk is data loss on OS crash / power loss,
        // which is acceptable for conversation-history data.
        diesel::sql_query("PRAGMA synchronous = NORMAL").execute(conn)?;
        // 8 MB page cache (-N means N kibibytes). Reduces I/O on repeated reads of
        // the same session/message rows.
        diesel::sql_query("PRAGMA cache_size = -8000").execute(conn)?;
        // Keep temporary tables in memory instead of on-disk temp files.
        diesel::sql_query("PRAGMA temp_store = MEMORY").execute(conn)?;
        Ok(())
    }

    fn default_path() -> PersistenceResult<PathBuf> {
        let home = dirs::home_dir().ok_or(PersistenceError::NoHomeDir)?;
        Ok(home.join(".opengoose").join("sessions.db"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = Database::open_in_memory().unwrap();
        // Verify we can execute a simple query
        db.with(|conn| {
            diesel::sql_query("SELECT 1").execute(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_open_at_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        assert!(!path.exists());
        let _db = Database::open_at(path.clone()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_open_at_creates_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("test.db");
        let _db = Database::open_at(path.clone()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_with_closure() {
        let db = Database::open_in_memory().unwrap();
        let result = db.with(|conn| {
            let val = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("42"))
                .get_result::<i32>(conn)?;
            Ok(val)
        });
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_migrations_create_tables() {
        let db = Database::open_in_memory().unwrap();
        // Verify that migration-created tables exist by querying them
        db.with(|conn| {
            diesel::sql_query("SELECT count(*) FROM sessions").execute(conn)?;
            diesel::sql_query("SELECT count(*) FROM messages").execute(conn)?;
            diesel::sql_query("SELECT count(*) FROM message_queue").execute(conn)?;
            diesel::sql_query("SELECT count(*) FROM work_items").execute(conn)?;
            diesel::sql_query("SELECT count(*) FROM orchestration_runs").execute(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_default_path_returns_sessions_db() {
        if let Ok(path) = Database::default_path() {
            assert!(path.ends_with("sessions.db"));
            assert!(path.to_string_lossy().contains(".opengoose"));
        }
    }

    #[test]
    fn test_now_sql_returns_datetime() {
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let result: String = diesel::select(now_sql()).get_result(conn)?;
            assert!(result.contains('-'));
            assert!(result.contains(':'));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_now_sql_nullable_returns_datetime() {
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let result: Option<String> = diesel::select(now_sql_nullable()).get_result(conn)?;
            assert!(result.is_some());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_open_at_same_path_twice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let _db1 = Database::open_at(path.clone()).unwrap();
        drop(_db1);
        let _db2 = Database::open_at(path).unwrap();
    }
}
