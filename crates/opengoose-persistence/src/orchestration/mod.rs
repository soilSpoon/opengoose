mod mutations;
mod queries;
#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;

use tracing::{debug, info};

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::run_status::RunStatus;

pub use types::OrchestrationRun;

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
            mutations::create_run(
                conn,
                team_run_id,
                session_key,
                team_name,
                workflow,
                input,
                total_steps,
            )
        })?;
        debug!(team_run_id, team_name, "orchestration run created");
        Ok(())
    }

    /// Advance the current step.
    pub fn advance_step(&self, team_run_id: &str, step: i32) -> PersistenceResult<()> {
        self.db
            .with(|conn| mutations::advance_step(conn, team_run_id, step))
    }

    /// Resume a suspended run, setting its status back to running.
    pub fn resume_run(&self, team_run_id: &str) -> PersistenceResult<()> {
        self.db
            .with(|conn| mutations::resume_run(conn, team_run_id))?;
        debug!(team_run_id, "orchestration run resumed");
        Ok(())
    }

    /// Mark a run as completed with a result.
    pub fn complete_run(&self, team_run_id: &str, result: &str) -> PersistenceResult<()> {
        self.db
            .with(|conn| mutations::complete_run(conn, team_run_id, result))?;
        debug!(team_run_id, "orchestration run completed");
        Ok(())
    }

    /// Mark a run as failed.
    pub fn fail_run(&self, team_run_id: &str, error: &str) -> PersistenceResult<()> {
        self.db
            .with(|conn| mutations::fail_run(conn, team_run_id, error))
    }

    /// Suspend all running runs (called on startup for crash recovery).
    pub fn suspend_incomplete(&self) -> PersistenceResult<usize> {
        let count = self.db.with(mutations::suspend_incomplete)?;
        if count > 0 {
            info!(count, "suspended incomplete orchestration runs");
        }
        Ok(count)
    }

    /// Get a run by team_run_id.
    pub fn get_run(&self, team_run_id: &str) -> PersistenceResult<Option<OrchestrationRun>> {
        self.db.with(|conn| queries::get_run(conn, team_run_id))
    }

    /// List runs, optionally filtered by status, limited to `limit` results.
    pub fn list_runs(
        &self,
        status: Option<&RunStatus>,
        limit: i64,
    ) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db.with(|conn| queries::list_runs(conn, status, limit))
    }

    /// Count all orchestration runs.
    pub fn count_runs(&self) -> PersistenceResult<i64> {
        self.db.with(queries::count_runs)
    }

    /// Find suspended runs for a session (for `/team resume`).
    pub fn find_suspended(&self, session_key: &str) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db
            .with(|conn| queries::find_suspended(conn, session_key))
    }
}
