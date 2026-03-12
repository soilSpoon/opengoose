use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use crate::error::{PersistenceError, PersistenceResult};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub(super) fn run_pending(conn: &mut SqliteConnection) -> PersistenceResult<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| PersistenceError::Migration(e.to_string()))?;
    Ok(())
}
