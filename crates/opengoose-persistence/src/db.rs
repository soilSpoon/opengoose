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
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
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

    /// Verify every table defined in schema.rs is created by migrations.
    #[test]
    fn test_migrations_create_all_schema_tables() {
        let db = Database::open_in_memory().unwrap();
        let tables = [
            "sessions",
            "messages",
            "message_queue",
            "work_items",
            "orchestration_runs",
            "alert_rules",
            "alert_history",
            "schedules",
            "agent_messages",
            "triggers",
            "plugins",
        ];
        db.with(|conn| {
            for table in &tables {
                diesel::sql_query(format!("SELECT count(*) FROM {table}"))
                    .execute(conn)
                    .unwrap_or_else(|_| panic!("table '{table}' should exist after migrations"));
            }
            Ok(())
        })
        .unwrap();
    }

    /// Running migrations a second time must be idempotent (no error).
    #[test]
    fn test_migration_idempotency() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idempotent.db");
        let _db1 = Database::open_at(path.clone()).unwrap();
        // Opening the same file again runs pending migrations (none left) — must succeed.
        let db2 = Database::open_at(path).unwrap();
        db2.with(|conn| {
            diesel::sql_query("SELECT count(*) FROM sessions").execute(conn)?;
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `sessions` table via PRAGMA.
    #[test]
    fn test_sessions_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(sessions)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            assert!(names.contains(&"id"));
            assert!(names.contains(&"session_key"));
            assert!(names.contains(&"active_team"));
            assert!(names.contains(&"created_at"));
            assert!(names.contains(&"updated_at"));
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `messages` table.
    #[test]
    fn test_messages_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(messages)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            assert!(names.contains(&"id"));
            assert!(names.contains(&"session_key"));
            assert!(names.contains(&"role"));
            assert!(names.contains(&"content"));
            assert!(names.contains(&"author"));
            assert!(names.contains(&"created_at"));
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `message_queue` table.
    #[test]
    fn test_message_queue_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols =
                diesel::sql_query("PRAGMA table_info(message_queue)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &[
                "session_key",
                "team_run_id",
                "sender",
                "recipient",
                "content",
                "msg_type",
                "status",
                "retry_count",
                "max_retries",
                "created_at",
            ] {
                assert!(names.contains(col), "message_queue missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `alert_rules` table.
    #[test]
    fn test_alert_rules_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(alert_rules)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &["id", "name", "metric", "condition", "threshold", "enabled"] {
                assert!(names.contains(col), "alert_rules missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `schedules` table.
    #[test]
    fn test_schedules_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(schedules)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &[
                "name",
                "cron_expression",
                "team_name",
                "input",
                "enabled",
                "last_run_at",
                "next_run_at",
            ] {
                assert!(names.contains(col), "schedules missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `triggers` table.
    #[test]
    fn test_triggers_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(triggers)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &[
                "name",
                "trigger_type",
                "condition_json",
                "team_name",
                "input",
                "enabled",
                "last_fired_at",
                "fire_count",
            ] {
                assert!(names.contains(col), "triggers missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `plugins` table.
    #[test]
    fn test_plugins_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols = diesel::sql_query("PRAGMA table_info(plugins)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &[
                "name",
                "version",
                "author",
                "description",
                "capabilities",
                "source_path",
                "enabled",
            ] {
                assert!(names.contains(col), "plugins missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// Verify key columns exist in the `agent_messages` table.
    #[test]
    fn test_agent_messages_table_columns() {
        #[derive(diesel::QueryableByName)]
        struct ColInfo {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            let cols =
                diesel::sql_query("PRAGMA table_info(agent_messages)").load::<ColInfo>(conn)?;
            let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
            for col in &[
                "session_key",
                "from_agent",
                "to_agent",
                "channel",
                "payload",
                "status",
                "created_at",
                "delivered_at",
            ] {
                assert!(names.contains(col), "agent_messages missing column '{col}'");
            }
            Ok(())
        })
        .unwrap();
    }

    /// PRAGMA foreign_keys=ON is active: inserting a message without a session row
    /// must fail due to the FK constraint.
    #[test]
    fn test_foreign_key_constraints_enforced() {
        let db = Database::open_in_memory().unwrap();
        let result = db.with(|conn| {
            // Intentionally omit a session row — FK should reject this.
            diesel::sql_query(
                "INSERT INTO messages (session_key, role, content, created_at) \
                 VALUES ('no-such-session', 'user', 'hello', datetime('now'))",
            )
            .execute(conn)?;
            Ok(())
        });
        // SQLite FK violation returns a DatabaseError
        assert!(
            result.is_err(),
            "FK constraint should reject orphan message"
        );
    }

    /// WAL journal mode pragma is applied — query it back.
    #[test]
    fn test_wal_journal_mode() {
        #[derive(diesel::QueryableByName)]
        struct JournalRow {
            #[diesel(column_name = journal_mode)]
            #[diesel(sql_type = diesel::sql_types::Text)]
            _journal_mode: String,
        }
        let db = Database::open_in_memory().unwrap();
        db.with(|conn| {
            // In-memory databases always report "memory" mode even when WAL is requested;
            // for file databases the pragma returns "wal".
            let rows: Vec<JournalRow> = diesel::sql_query("PRAGMA journal_mode").load(conn)?;
            let mode = rows
                .into_iter()
                .next()
                .expect("journal_mode pragma should return one row")
                .journal_mode;
            assert!(mode == "memory" || mode == "wal");
            Ok(())
        })
        .unwrap();
    }

    /// `with` propagates errors correctly.
    #[test]
    fn test_with_propagates_error() {
        let db = Database::open_in_memory().unwrap();
        let result = db.with(|conn| {
            diesel::sql_query("SELECT * FROM nonexistent_table_xyz").execute(conn)?;
            Ok(())
        });
        assert!(result.is_err());
    }

    /// `with` returns `LockPoisoned` when the underlying mutex has been poisoned.
    #[test]
    fn test_with_returns_lock_poisoned_on_poisoned_mutex() {
        let db = std::sync::Arc::new(Database::open_in_memory().unwrap());
        let db_clone = db.clone();

        // Poison the mutex by panicking inside a `with` closure on another thread.
        let _ = std::thread::spawn(move || {
            let _ = db_clone.with(|_conn| -> PersistenceResult<()> {
                panic!("intentional panic to poison the mutex");
            });
        })
        .join();

        // The mutex is now poisoned — `with` should return `LockPoisoned`.
        let result = db.with(|conn| {
            diesel::sql_query("SELECT 1").execute(conn)?;
            Ok(())
        });
        assert!(
            matches!(result, Err(PersistenceError::LockPoisoned)),
            "expected LockPoisoned, got {result:?}"
        );
    }

    /// Multiple `open_at` calls on the same path succeed (migrations idempotent on file db).
    #[test]
    fn test_open_at_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.db");
        let db1 = Database::open_at(path.clone()).unwrap();
        drop(db1);
        // Second open: migrations already applied, must not error
        let db2 = Database::open_at(path).unwrap();
        db2.with(|conn| {
            diesel::sql_query("SELECT count(*) FROM sessions").execute(conn)?;
            Ok(())
        })
        .unwrap();
    }
}
