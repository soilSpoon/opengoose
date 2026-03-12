use super::*;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

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
