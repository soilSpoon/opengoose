use super::*;

use diesel::prelude::*;

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
