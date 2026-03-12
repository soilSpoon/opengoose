use super::*;

use diesel::prelude::*;

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
