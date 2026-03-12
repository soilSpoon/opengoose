//! Shared test utilities for the persistence crate.
//!
//! Avoids duplicating `test_db()` and `ensure_session()` across every test module.

use std::sync::Arc;

use diesel::prelude::*;

use crate::db::Database;
use crate::models::NewSession;
use crate::schema::sessions;

/// Create an in-memory database for testing.
pub(crate) fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

/// Ensure a session row exists so FK constraints are satisfied.
pub(crate) fn ensure_session(db: &Arc<Database>, key: &str) {
    db.with(|conn| {
        diesel::insert_into(sessions::table)
            .values(NewSession {
                session_key: key,
                selected_model: None,
            })
            .on_conflict(sessions::session_key)
            .do_nothing()
            .execute(conn)?;
        Ok(())
    })
    .unwrap();
}
