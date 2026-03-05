use std::sync::Arc;

use diesel::prelude::*;
use tracing::{debug, info};

use crate::db::{self, Database};
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewOrchestrationRun, OrchestrationRunRow};
use crate::schema::orchestration_runs;

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

    pub fn from_str(s: &str) -> Result<Self, PersistenceError> {
        match s {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "suspended" => Ok(Self::Suspended),
            other => Err(PersistenceError::InvalidEnumValue(format!(
                "unknown RunStatus: {other}"
            ))),
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

impl OrchestrationRun {
    fn from_row(row: OrchestrationRunRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            status: RunStatus::from_str(&row.status)?,
            team_run_id: row.team_run_id,
            session_key: row.session_key,
            team_name: row.team_name,
            workflow: row.workflow,
            input: row.input,
            current_step: row.current_step,
            total_steps: row.total_steps,
            result: row.result,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
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
            diesel::insert_into(orchestration_runs::table)
                .values(NewOrchestrationRun {
                    team_run_id,
                    session_key,
                    team_name,
                    workflow,
                    input,
                    total_steps,
                })
                .execute(conn)?;
            debug!(team_run_id, team_name, "orchestration run created");
            Ok(())
        })
    }

    /// Advance the current step.
    pub fn advance_step(&self, team_run_id: &str, step: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                orchestration_runs::table
                    .filter(orchestration_runs::team_run_id.eq(team_run_id)),
            )
            .set((
                orchestration_runs::current_step.eq(step),
                orchestration_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            Ok(())
        })
    }

    /// Resume a suspended run, setting its status back to running.
    pub fn resume_run(&self, team_run_id: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                orchestration_runs::table
                    .filter(orchestration_runs::team_run_id.eq(team_run_id)),
            )
            .set((
                orchestration_runs::status.eq(RunStatus::Running.as_str()),
                orchestration_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            debug!(team_run_id, "orchestration run resumed");
            Ok(())
        })
    }

    /// Mark a run as completed with a result.
    pub fn complete_run(&self, team_run_id: &str, result: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                orchestration_runs::table
                    .filter(orchestration_runs::team_run_id.eq(team_run_id)),
            )
            .set((
                orchestration_runs::status.eq(RunStatus::Completed.as_str()),
                orchestration_runs::result.eq(Some(result)),
                orchestration_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            debug!(team_run_id, "orchestration run completed");
            Ok(())
        })
    }

    /// Mark a run as failed.
    pub fn fail_run(&self, team_run_id: &str, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                orchestration_runs::table
                    .filter(orchestration_runs::team_run_id.eq(team_run_id)),
            )
            .set((
                orchestration_runs::status.eq(RunStatus::Failed.as_str()),
                orchestration_runs::result.eq(Some(error)),
                orchestration_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            Ok(())
        })
    }

    /// Suspend all running runs (called on startup for crash recovery).
    pub fn suspend_incomplete(&self) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let count = diesel::update(
                orchestration_runs::table
                    .filter(orchestration_runs::status.eq(RunStatus::Running.as_str())),
            )
            .set((
                orchestration_runs::status.eq(RunStatus::Suspended.as_str()),
                orchestration_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            if count > 0 {
                info!(count, "suspended incomplete orchestration runs");
            }
            Ok(count)
        })
    }

    /// Get a run by team_run_id.
    pub fn get_run(&self, team_run_id: &str) -> PersistenceResult<Option<OrchestrationRun>> {
        self.db.with(|conn| {
            let result = orchestration_runs::table
                .filter(orchestration_runs::team_run_id.eq(team_run_id))
                .first::<OrchestrationRunRow>(conn)
                .optional()?;
            match result {
                Some(row) => Ok(Some(OrchestrationRun::from_row(row)?)),
                None => Ok(None),
            }
        })
    }

    /// Find suspended runs for a session (for `/team resume`).
    pub fn find_suspended(&self, session_key: &str) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db.with(|conn| {
            let rows = orchestration_runs::table
                .filter(orchestration_runs::session_key.eq(session_key))
                .filter(orchestration_runs::status.eq(RunStatus::Suspended.as_str()))
                .order(orchestration_runs::updated_at.desc())
                .load::<OrchestrationRunRow>(conn)?;
            rows.into_iter()
                .map(OrchestrationRun::from_row)
                .collect::<Result<_, _>>()
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
        assert_eq!(suspended, 1);

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
