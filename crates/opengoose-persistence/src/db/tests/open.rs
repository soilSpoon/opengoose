use super::*;

use diesel::prelude::*;

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
fn test_open_at_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopen.db");
    let _db1 = Database::open_at(path.clone()).unwrap();
    let _db2 = Database::open_at(path).unwrap();
}

#[test]
fn test_default_path() {
    if let Ok(path) = Database::default_path() {
        assert!(path.ends_with("sessions.db"));
        assert!(path.to_string_lossy().contains(".opengoose"));
    }
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
fn test_with_propagates_error() {
    let db = Database::open_in_memory().unwrap();
    let result = db.with(|conn| {
        diesel::sql_query("SELECT * FROM nonexistent_table_xyz").execute(conn)?;
        Ok(())
    });

    assert!(result.is_err());
}

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
