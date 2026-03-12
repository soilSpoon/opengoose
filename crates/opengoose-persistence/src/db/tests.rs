use super::*;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use crate::error::{PersistenceError, PersistenceResult};

#[derive(diesel::QueryableByName)]
struct ColInfo {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

fn load_column_names(conn: &mut SqliteConnection, table: &str) -> PersistenceResult<Vec<String>> {
    Ok(diesel::sql_query(format!("PRAGMA table_info({table})"))
        .load::<ColInfo>(conn)?
        .into_iter()
        .map(|col| col.name)
        .collect())
}

fn assert_table_has_columns(db: &Database, table: &str, expected: &[&str]) {
    db.with(|conn| {
        let names = load_column_names(conn, table)?;
        for column in expected {
            assert!(
                names.iter().any(|name| name == column),
                "{table} missing column '{column}'"
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_open_in_memory() {
    let db = Database::open_in_memory().unwrap();
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
        "event_history",
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
    let db2 = Database::open_at(path).unwrap();
    db2.with(|conn| {
        diesel::sql_query("SELECT count(*) FROM sessions").execute(conn)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_sessions_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "sessions",
        &[
            "id",
            "session_key",
            "active_team",
            "selected_model",
            "created_at",
            "updated_at",
        ],
    );
}

#[test]
fn test_messages_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "messages",
        &[
            "id",
            "session_key",
            "role",
            "content",
            "author",
            "created_at",
        ],
    );
}

#[test]
fn test_message_queue_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "message_queue",
        &[
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
        ],
    );
}

#[test]
fn test_alert_rules_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "alert_rules",
        &["id", "name", "metric", "condition", "threshold", "enabled"],
    );
}

#[test]
fn test_schedules_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "schedules",
        &[
            "name",
            "cron_expression",
            "team_name",
            "input",
            "enabled",
            "last_run_at",
            "next_run_at",
        ],
    );
}

#[test]
fn test_triggers_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "triggers",
        &[
            "name",
            "trigger_type",
            "condition_json",
            "team_name",
            "input",
            "enabled",
            "last_fired_at",
            "fire_count",
        ],
    );
}

#[test]
fn test_plugins_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "plugins",
        &[
            "name",
            "version",
            "author",
            "description",
            "capabilities",
            "source_path",
            "enabled",
        ],
    );
}

#[test]
fn test_agent_messages_table_columns() {
    let db = Database::open_in_memory().unwrap();
    assert_table_has_columns(
        &db,
        "agent_messages",
        &[
            "session_key",
            "from_agent",
            "to_agent",
            "channel",
            "payload",
            "status",
            "created_at",
            "delivered_at",
        ],
    );
}

/// PRAGMA foreign_keys=ON is active: inserting a message without a session row
/// must fail due to the FK constraint.
#[test]
fn test_foreign_key_constraints_enforced() {
    let db = Database::open_in_memory().unwrap();
    let result = db.with(|conn| {
        diesel::sql_query(
            "INSERT INTO messages (session_key, role, content, created_at) \
             VALUES ('no-such-session', 'user', 'hello', datetime('now'))",
        )
        .execute(conn)?;
        Ok(())
    });
    assert!(
        result.is_err(),
        "FK constraint should reject orphan message"
    );
}

/// WAL journal mode pragma is applied.
#[test]
fn test_wal_journal_mode() {
    #[derive(diesel::QueryableByName)]
    struct JournalRow {
        #[diesel(column_name = journal_mode)]
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
    }

    let db = Database::open_in_memory().unwrap();
    db.with(|conn| {
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

/// File-backed databases should keep WAL mode and foreign key enforcement enabled.
#[test]
fn test_file_database_pragmas() {
    #[derive(diesel::QueryableByName)]
    struct JournalRow {
        #[diesel(column_name = journal_mode)]
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
    }

    #[derive(diesel::QueryableByName)]
    struct ForeignKeysRow {
        #[diesel(column_name = foreign_keys)]
        #[diesel(sql_type = diesel::sql_types::Integer)]
        foreign_keys: i32,
    }

    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_at(dir.path().join("pragmas.db")).unwrap();
    db.with(|conn| {
        let journal_mode = diesel::sql_query("PRAGMA journal_mode")
            .load::<JournalRow>(conn)?
            .into_iter()
            .next()
            .expect("journal_mode pragma should return one row")
            .journal_mode;
        let foreign_keys = diesel::sql_query("PRAGMA foreign_keys")
            .load::<ForeignKeysRow>(conn)?
            .into_iter()
            .next()
            .expect("foreign_keys pragma should return one row")
            .foreign_keys;
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
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

    let _ = std::thread::spawn(move || {
        let _ = db_clone.with(|_conn| -> PersistenceResult<()> {
            panic!("intentional panic to poison the mutex");
        });
    })
    .join();

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
    let db2 = Database::open_at(path).unwrap();
    db2.with(|conn| {
        diesel::sql_query("SELECT count(*) FROM sessions").execute(conn)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_default_path() {
    if let Ok(path) = Database::default_path() {
        assert!(path.ends_with("sessions.db"));
        assert!(path.to_string_lossy().contains(".opengoose"));
    }
}

#[test]
fn test_open_at_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopen.db");
    let _db1 = Database::open_at(path.clone()).unwrap();
    let _db2 = Database::open_at(path).unwrap();
}

#[test]
fn test_now_sql_helpers() {
    let db = Database::open_in_memory().unwrap();
    db.with(|conn| {
        let result = diesel::select(now_sql()).get_result::<String>(conn)?;
        assert!(!result.is_empty());

        let result = diesel::select(now_sql_nullable()).get_result::<Option<String>>(conn)?;
        assert!(result.is_some());
        Ok(())
    })
    .unwrap();
}
