diesel::table! {
    sessions (id) {
        id -> Integer,
        session_key -> Text,
        active_team -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        session_key -> Text,
        role -> Text,
        content -> Text,
        author -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    message_queue (id) {
        id -> Integer,
        session_key -> Text,
        team_run_id -> Text,
        sender -> Text,
        recipient -> Text,
        content -> Text,
        msg_type -> Text,
        status -> Text,
        retry_count -> Integer,
        max_retries -> Integer,
        created_at -> Text,
        processed_at -> Nullable<Text>,
        error -> Nullable<Text>,
    }
}

diesel::table! {
    work_items (id) {
        id -> Integer,
        session_key -> Text,
        team_run_id -> Text,
        parent_id -> Nullable<Integer>,
        title -> Text,
        description -> Nullable<Text>,
        status -> Text,
        assigned_to -> Nullable<Text>,
        workflow_step -> Nullable<Integer>,
        input -> Nullable<Text>,
        output -> Nullable<Text>,
        error -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    orchestration_runs (id) {
        id -> Integer,
        team_run_id -> Text,
        session_key -> Text,
        team_name -> Text,
        workflow -> Text,
        input -> Text,
        status -> Text,
        current_step -> Integer,
        total_steps -> Integer,
        result -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    alert_rules (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        metric -> Text,
        condition -> Text,
        threshold -> Double,
        enabled -> Integer,
        actions -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    alert_history (id) {
        id -> Integer,
        rule_id -> Text,
        rule_name -> Text,
        metric -> Text,
        value -> Double,
        triggered_at -> Text,
    }
}

diesel::table! {
    event_history (id) {
        id -> Integer,
        event_kind -> Text,
        timestamp -> Text,
        source_gateway -> Nullable<Text>,
        session_key -> Nullable<Text>,
        payload -> Text,
    }
}

diesel::table! {
    schedules (id) {
        id -> Integer,
        name -> Text,
        cron_expression -> Text,
        team_name -> Text,
        input -> Text,
        enabled -> Integer,
        last_run_at -> Nullable<Text>,
        next_run_at -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    agent_messages (id) {
        id -> Integer,
        session_key -> Text,
        from_agent -> Text,
        to_agent -> Nullable<Text>,
        channel -> Nullable<Text>,
        payload -> Text,
        status -> Text,
        created_at -> Text,
        delivered_at -> Nullable<Text>,
    }
}

diesel::table! {
    triggers (id) {
        id -> Integer,
        name -> Text,
        trigger_type -> Text,
        condition_json -> Text,
        team_name -> Text,
        input -> Text,
        enabled -> Integer,
        last_fired_at -> Nullable<Text>,
        fire_count -> Integer,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    plugins (id) {
        id -> Integer,
        name -> Text,
        version -> Text,
        author -> Nullable<Text>,
        description -> Nullable<Text>,
        capabilities -> Text,
        source_path -> Text,
        enabled -> Integer,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    api_keys (id) {
        id -> Text,
        key_hash -> Text,
        description -> Nullable<Text>,
        created_at -> Text,
        last_used_at -> Nullable<Text>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    messages,
    message_queue,
    work_items,
    orchestration_runs,
    alert_rules,
    alert_history,
    event_history,
    schedules,
    agent_messages,
    triggers,
    plugins,
    api_keys,
);

#[cfg(test)]
mod tests {
    use diesel::prelude::*;

    use crate::db::Database;

    #[derive(diesel::QueryableByName)]
    struct ColInfo {
        #[diesel(sql_type = diesel::sql_types::Text)]
        name: String,
        #[diesel(sql_type = diesel::sql_types::Integer)]
        notnull: i32,
    }

    /// Helper: load column info for a table.
    fn column_info(db: &Database, table: &str) -> Vec<ColInfo> {
        db.with(|conn| {
            let cols =
                diesel::sql_query(format!("PRAGMA table_info({table})")).load::<ColInfo>(conn)?;
            Ok(cols)
        })
        .unwrap()
    }

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    // ── work_items columns (not verified elsewhere) ──

    #[test]
    fn test_work_items_table_columns() {
        let db = test_db();
        let cols = column_info(&db, "work_items");
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        for col in &[
            "id",
            "session_key",
            "team_run_id",
            "parent_id",
            "title",
            "description",
            "status",
            "assigned_to",
            "workflow_step",
            "input",
            "output",
            "error",
            "created_at",
            "updated_at",
        ] {
            assert!(names.contains(col), "work_items missing column '{col}'");
        }
    }

    #[test]
    fn test_work_items_nullable_columns() {
        let db = test_db();
        let cols = column_info(&db, "work_items");
        let nullable: Vec<&str> = cols
            .iter()
            .filter(|c| c.notnull == 0)
            .map(|c| c.name.as_str())
            .collect();
        for col in &[
            "parent_id",
            "description",
            "assigned_to",
            "workflow_step",
            "input",
            "output",
            "error",
        ] {
            assert!(
                nullable.contains(col),
                "work_items column '{col}' should be nullable"
            );
        }
    }

    // ── orchestration_runs columns (not verified elsewhere) ──

    #[test]
    fn test_orchestration_runs_table_columns() {
        let db = test_db();
        let cols = column_info(&db, "orchestration_runs");
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        for col in &[
            "id",
            "team_run_id",
            "session_key",
            "team_name",
            "workflow",
            "input",
            "status",
            "current_step",
            "total_steps",
            "result",
            "created_at",
            "updated_at",
        ] {
            assert!(
                names.contains(col),
                "orchestration_runs missing column '{col}'"
            );
        }
    }

    #[test]
    fn test_orchestration_runs_nullable_columns() {
        let db = test_db();
        let cols = column_info(&db, "orchestration_runs");
        let nullable: Vec<&str> = cols
            .iter()
            .filter(|c| c.notnull == 0)
            .map(|c| c.name.as_str())
            .collect();
        assert!(
            nullable.contains(&"result"),
            "orchestration_runs.result should be nullable"
        );
    }

    // ── alert_history columns (not verified elsewhere) ──

    #[test]
    fn test_alert_history_table_columns() {
        let db = test_db();
        let cols = column_info(&db, "alert_history");
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        for col in &[
            "id",
            "rule_id",
            "rule_name",
            "metric",
            "value",
            "triggered_at",
        ] {
            assert!(names.contains(col), "alert_history missing column '{col}'");
        }
    }

    #[test]
    fn test_event_history_table_columns() {
        let db = test_db();
        let cols = column_info(&db, "event_history");
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        for col in &[
            "id",
            "event_kind",
            "timestamp",
            "source_gateway",
            "session_key",
            "payload",
        ] {
            assert!(names.contains(col), "event_history missing column '{col}'");
        }
    }

    // ── Column count verification ──

    #[test]
    fn test_sessions_column_count() {
        let db = test_db();
        let cols = column_info(&db, "sessions");
        assert_eq!(cols.len(), 5, "sessions table should have 5 columns");
    }

    #[test]
    fn test_messages_column_count() {
        let db = test_db();
        let cols = column_info(&db, "messages");
        assert_eq!(cols.len(), 6, "messages table should have 6 columns");
    }

    #[test]
    fn test_message_queue_column_count() {
        let db = test_db();
        let cols = column_info(&db, "message_queue");
        assert_eq!(cols.len(), 13, "message_queue table should have 13 columns");
    }

    #[test]
    fn test_work_items_column_count() {
        let db = test_db();
        let cols = column_info(&db, "work_items");
        assert_eq!(cols.len(), 14, "work_items table should have 14 columns");
    }

    #[test]
    fn test_orchestration_runs_column_count() {
        let db = test_db();
        let cols = column_info(&db, "orchestration_runs");
        assert_eq!(
            cols.len(),
            12,
            "orchestration_runs table should have 12 columns"
        );
    }

    #[test]
    fn test_event_history_column_count() {
        let db = test_db();
        let cols = column_info(&db, "event_history");
        assert_eq!(cols.len(), 6, "event_history table should have 6 columns");
    }

    #[test]
    fn test_schedules_column_count() {
        let db = test_db();
        let cols = column_info(&db, "schedules");
        assert_eq!(cols.len(), 10, "schedules table should have 10 columns");
    }

    #[test]
    fn test_plugins_column_count() {
        let db = test_db();
        let cols = column_info(&db, "plugins");
        assert_eq!(cols.len(), 10, "plugins table should have 10 columns");
    }

    // ── Nullable columns for message_queue match schema definition ──

    #[test]
    fn test_message_queue_nullable_columns() {
        let db = test_db();
        let cols = column_info(&db, "message_queue");
        let nullable: Vec<&str> = cols
            .iter()
            .filter(|c| c.notnull == 0)
            .map(|c| c.name.as_str())
            .collect();
        for col in &["processed_at", "error"] {
            assert!(
                nullable.contains(col),
                "message_queue column '{col}' should be nullable"
            );
        }
    }

    // ── Cross-table query compilation (allow_tables_to_appear_in_same_query) ──

    #[test]
    fn test_sessions_messages_join_compiles() {
        use super::{messages, sessions};

        let db = test_db();
        db.with(|conn| {
            let _rows = sessions::table
                .inner_join(messages::table.on(messages::session_key.eq(sessions::session_key)))
                .select((sessions::session_key, messages::content))
                .load::<(String, String)>(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_sessions_message_queue_join_compiles() {
        use super::{message_queue, sessions};

        let db = test_db();
        db.with(|conn| {
            let _rows = sessions::table
                .inner_join(
                    message_queue::table.on(message_queue::session_key.eq(sessions::session_key)),
                )
                .select((sessions::session_key, message_queue::content))
                .load::<(String, String)>(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_alert_rules_alert_history_join_compiles() {
        use super::{alert_history, alert_rules};

        let db = test_db();
        db.with(|conn| {
            let _rows = alert_rules::table
                .inner_join(alert_history::table.on(alert_history::rule_id.eq(alert_rules::id)))
                .select((alert_rules::name, alert_history::value))
                .load::<(String, f64)>(conn)?;
            Ok(())
        })
        .unwrap();
    }
}
