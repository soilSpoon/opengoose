use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::info;

use crate::error::{PersistenceError, PersistenceResult};
use crate::schema;

/// Shared SQLite database wrapper.
///
/// All persistence modules (sessions, message queue, work items, orchestration runs)
/// share the same `Arc<Database>` to operate on a single connection.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open or create the database at `~/.opengoose/sessions.db`.
    pub fn open() -> PersistenceResult<Self> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open or create the database at a specific path.
    pub fn open_at(path: PathBuf) -> PersistenceResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        schema::initialize(&conn)?;
        info!(path = %path.display(), "database opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> PersistenceResult<Self> {
        let conn = Connection::open_in_memory()?;
        schema::initialize(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Execute a closure with access to the underlying connection.
    pub fn with<F, T>(&self, f: F) -> PersistenceResult<T>
    where
        F: FnOnce(&Connection) -> PersistenceResult<T>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }

    fn default_path() -> PersistenceResult<PathBuf> {
        let home = dirs::home_dir().ok_or(PersistenceError::NoHomeDir)?;
        Ok(home.join(".opengoose").join("sessions.db"))
    }
}
