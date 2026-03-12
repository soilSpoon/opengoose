use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use crate::error::PersistenceResult;

pub(super) fn configure(conn: &mut SqliteConnection) -> PersistenceResult<()> {
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
    // Enable memory-mapped I/O for up to 128 MiB of the database file.
    // mmap reads bypass the kernel's read(2) syscall path, reducing syscall
    // overhead for repeated page accesses. Safe to use alongside WAL mode.
    diesel::sql_query("PRAGMA mmap_size = 134217728").execute(conn)?;
    Ok(())
}
