use std::sync::Arc;

use rusqlite::params;
use tracing::{debug, info};

use crate::db::Database;
use crate::error::PersistenceResult;

/// Status of an orchestration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Suspended,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Suspended => "suspended",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "suspended" => Self::Suspended,
            _ => Self::Running,
        }
    }
}

/// A tracked orchestration run (for crash recovery).
#[derive(Debug, Clone)]
pub struct OrchestrationRun {
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub input: String,
    pub status: RunStatus,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Orchestration run tracking on a shared Database.
pub struct OrchestrationStore {
    db: Arc<Database>,
}

impl OrchestrationStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new orchestration run.
    pub fn create_run(
        &self,
        team_run_id: &str,
        session_key: &str,
        team_name: &str,
        workflow: &str,
        input: &str,
        total_steps: i32,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO orchestration_runs (team_run_id, session_key, team_name, workflow, input, total_steps)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![team_run_id, session_key, team_name, workflow, input, total_steps],
            )?;
            debug!(team_run_id, team_name, "orchestration run created");
            Ok(())
        })
    }

    /// Advance the current step.
    pub fn advance_step(&self, team_run_id: &str, step: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE orchestration_runs SET current_step = ?1, updated_at = datetime('now') WHERE team_run_id = ?2",
                params![step, team_run_id],
            )?;
            Ok(())
        })
    }

    /// Mark a run as completed with a result.
    pub fn complete_run(&self, team_run_id: &str, result: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE orchestration_runs SET status = 'completed', result = ?1, updated_at = datetime('now') WHERE team_run_id = ?2",
                params![result, team_run_id],
            )?;
            debug!(team_run_id, "orchestration run completed");
            Ok(())
        })
    }

    /// Mark a run as failed.
    pub fn fail_run(&self, team_run_id: &str, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE orchestration_runs SET status = 'failed', result = ?1, updated_at = datetime('now') WHERE team_run_id = ?2",
                params![error, team_run_id],
            )?;
            Ok(())
        })
    }

    /// Suspend all running runs (called on startup for crash recovery).
    pub fn suspend_incomplete(&self) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let count = conn.execute(
                "UPDATE orchestration_runs SET status = 'suspended', updated_at = datetime('now') WHERE status = 'running'",
                [],
            )?;
            if count > 0 {
                info!(count, "suspended incomplete orchestration runs");
            }
            Ok(count)
        })
    }

    /// Get a run by team_run_id.
    pub fn get_run(&self, team_run_id: &str) -> PersistenceResult<Option<OrchestrationRun>> {
        self.db.with(|conn| {
            let result = conn.query_row(
                "SELECT team_run_id, session_key, team_name, workflow, input, status,
                        current_step, total_steps, result, created_at, updated_at
                 FROM orchestration_runs WHERE team_run_id = ?1",
                params![team_run_id],
                Self::row_to_run,
            );
            match result {
                Ok(run) => Ok(Some(run)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Find suspended runs for a session (for `/team resume`).
    pub fn find_suspended(&self, session_key: &str) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT team_run_id, session_key, team_name, workflow, input, status,
                        current_step, total_steps, result, created_at, updated_at
                 FROM orchestration_runs
                 WHERE session_key = ?1 AND status = 'suspended'
                 ORDER BY updated_at DESC",
            )?;
            let runs: Vec<OrchestrationRun> = stmt
                .query_map(params![session_key], Self::row_to_run)?
                .collect::<Result<_, _>>()?;
            Ok(runs)
        })
    }

    fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrchestrationRun> {
        Ok(OrchestrationRun {
            team_run_id: row.get(0)?,
            session_key: row.get(1)?,
            team_name: row.get(2)?,
            workflow: row.get(3)?,
            input: row.get(4)?,
            status: RunStatus::from_str(&row.get::<_, String>(5)?),
            current_step: row.get(6)?,
            total_steps: row.get(7)?,
            result: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn test_create_and_get() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "code-review", "chain", "review this PR", 3)
            .unwrap();

        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.team_name, "code-review");
        assert_eq!(run.workflow, "chain");
        assert_eq!(run.status, RunStatus::Running);
        assert_eq!(run.total_steps, 3);
    }

    #[test]
    fn test_advance_and_complete() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "review", "chain", "input", 2)
            .unwrap();

        store.advance_step("run1", 1).unwrap();
        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.current_step, 1);

        store.complete_run("run1", "all good").unwrap();
        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.result.as_deref(), Some("all good"));
    }

    #[test]
    fn test_suspend_incomplete() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "t1", "chain", "i1", 2)
            .unwrap();
        store
            .create_run("run2", "sess1", "t2", "fan_out", "i2", 3)
            .unwrap();
        store.complete_run("run2", "done").unwrap();

        let suspended = store.suspend_incomplete().unwrap();
        assert_eq!(suspended, 1); // only run1 was running

        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Suspended);
    }

    #[test]
    fn test_find_suspended() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "t1", "chain", "i1", 2)
            .unwrap();
        store
            .create_run("run2", "sess2", "t2", "chain", "i2", 2)
            .unwrap();
        store.suspend_incomplete().unwrap();

        let runs = store.find_suspended("sess1").unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].team_run_id, "run1");
    }
}
