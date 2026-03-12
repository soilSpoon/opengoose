use std::sync::Arc;

use diesel::prelude::*;
use tracing::{debug, info};

use crate::db::{self, Database};
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewOrchestrationRun, NewSession, OrchestrationRunRow};
use crate::run_status::RunStatus;
use crate::schema::{orchestration_runs, sessions};

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
            status: RunStatus::parse(&row.status)?,
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
            conn.transaction(|conn| {
                diesel::insert_into(sessions::table)
                    .values(NewSession {
                        session_key,
                        selected_model: None,
                    })
                    .on_conflict(sessions::session_key)
                    .do_nothing()
                    .execute(conn)?;

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
                Ok::<(), PersistenceError>(())
            })?;
            debug!(team_run_id, team_name, "orchestration run created");
            Ok(())
        })
    }

    /// Advance the current step.
    pub fn advance_step(&self, team_run_id: &str, step: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
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
                orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
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
                orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
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
                orchestration_runs::table.filter(orchestration_runs::team_run_id.eq(team_run_id)),
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
                .select(OrchestrationRunRow::as_select())
                .first(conn)
                .optional()?;
            match result {
                Some(row) => Ok(Some(OrchestrationRun::from_row(row)?)),
                None => Ok(None),
            }
        })
    }

    /// List runs, optionally filtered by status, limited to `limit` results.
    pub fn list_runs(
        &self,
        status: Option<&RunStatus>,
        limit: i64,
    ) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db.with(|conn| {
            let mut query = orchestration_runs::table
                .select(OrchestrationRunRow::as_select())
                .order(orchestration_runs::updated_at.desc())
                .limit(limit)
                .into_boxed();
            if let Some(s) = status {
                query = query.filter(orchestration_runs::status.eq(s.as_str()));
            }
            let rows = query.load::<OrchestrationRunRow>(conn)?;
            rows.into_iter()
                .map(OrchestrationRun::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Find suspended runs for a session (for `/team resume`).
    pub fn find_suspended(&self, session_key: &str) -> PersistenceResult<Vec<OrchestrationRun>> {
        self.db.with(|conn| {
            let rows = orchestration_runs::table
                .filter(orchestration_runs::session_key.eq(session_key))
                .filter(orchestration_runs::status.eq(RunStatus::Suspended.as_str()))
                .order(orchestration_runs::updated_at.desc())
                .select(OrchestrationRunRow::as_select())
                .load(conn)?;
            rows.into_iter()
                .map(OrchestrationRun::from_row)
                .collect::<Result<_, _>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewSession;
    use crate::schema::sessions;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn ensure_session(db: &Arc<Database>, key: &str) {
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

    #[test]
    fn test_create_and_get() {
        let db = test_db();
        ensure_session(&db, "sess1");
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
        ensure_session(&db, "sess1");
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
        ensure_session(&db, "sess1");
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
        ensure_session(&db, "sess1");
        ensure_session(&db, "sess2");
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

    #[test]
    fn test_fail_run() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "review", "chain", "input", 2)
            .unwrap();

        store.fail_run("run1", "agent crashed").unwrap();
        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.result.as_deref(), Some("agent crashed"));
    }

    #[test]
    fn test_resume_run() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "t1", "chain", "i1", 3)
            .unwrap();
        store.suspend_incomplete().unwrap();

        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Suspended);

        store.resume_run("run1").unwrap();
        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn test_get_run_nonexistent() {
        let db = test_db();
        let store = OrchestrationStore::new(db);
        let result = store.get_run("no-such-run").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_runs_filtered_by_status() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "t1", "chain", "i1", 2)
            .unwrap();
        store
            .create_run("run2", "sess1", "t2", "fan_out", "i2", 3)
            .unwrap();
        store.complete_run("run1", "done").unwrap();

        let running = store.list_runs(Some(&RunStatus::Running), 100).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].team_run_id, "run2");

        let completed = store.list_runs(Some(&RunStatus::Completed), 100).unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].team_run_id, "run1");

        let all = store.list_runs(None, 100).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_list_runs_respects_limit() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        for i in 0..5 {
            store
                .create_run(&format!("run{i}"), "sess1", "t", "chain", "i", 1)
                .unwrap();
        }

        let limited = store.list_runs(None, 3).unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[test]
    fn test_create_run_auto_creates_session() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        // Should not fail even without pre-creating the session
        store
            .create_run("run1", "new-sess", "team", "chain", "input", 2)
            .unwrap();

        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.session_key, "new-sess");
    }

    #[test]
    fn test_advance_step_then_complete() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "review", "chain", "input", 3)
            .unwrap();

        store.advance_step("run1", 1).unwrap();
        store.advance_step("run1", 2).unwrap();

        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.current_step, 2);
        assert_eq!(run.status, RunStatus::Running);

        store.complete_run("run1", "all done").unwrap();
        let run = store.get_run("run1").unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Completed);
    }

    #[test]
    fn test_find_suspended_empty() {
        let db = test_db();
        let store = OrchestrationStore::new(db);

        let runs = store.find_suspended("nonexistent-session").unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn test_suspend_incomplete_no_running_runs() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = OrchestrationStore::new(db);

        store
            .create_run("run1", "sess1", "t1", "chain", "i1", 2)
            .unwrap();
        store.complete_run("run1", "done").unwrap();

        // No running runs to suspend
        let count = store.suspend_incomplete().unwrap();
        assert_eq!(count, 0);
    }
}
