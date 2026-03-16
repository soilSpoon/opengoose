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
        "api_keys",
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

/// Verify that all performance indexes defined in migrations exist after a
/// fresh migration run. Missing indexes cause silent query-plan regressions
/// (full-table scan instead of index scan) with no functional test failure.
#[test]
fn test_performance_indexes_exist() {
    #[derive(diesel::QueryableByName)]
    struct IndexRow {
        #[diesel(sql_type = diesel::sql_types::Text)]
        name: String,
    }

    let db = Database::open_in_memory().unwrap();
    let expected = [
        // 2024-01-08-000000_add_performance_indexes
        "idx_schedules_enabled_next_run",
        "idx_triggers_enabled_type",
        // 2026-03-14-000000_add_listing_sort_indexes
        "idx_sessions_updated_at",
        "idx_or_updated_at",
        "idx_or_status_updated_at",
    ];

    db.with(|conn| {
        let present: Vec<String> = diesel::sql_query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'",
        )
        .load::<IndexRow>(conn)?
        .into_iter()
        .map(|r| r.name)
        .collect();

        for idx in &expected {
            assert!(
                present.iter().any(|n| n == idx),
                "performance index '{idx}' missing after migrations (present: {present:?})"
            );
        }
        Ok(())
    })
    .unwrap();
}
