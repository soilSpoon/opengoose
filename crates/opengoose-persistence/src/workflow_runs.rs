use std::sync::Arc;

use diesel::prelude::*;
use tracing::{debug, info};

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewWorkflowRun, WorkflowRunRow};
use crate::orchestration::RunStatus;
use crate::schema::workflow_runs;

/// Workflow run tracking on a shared Database.
///
/// Parallel to `OrchestrationStore` for teams — stores full workflow state
/// as JSON for crash recovery and resume support.
pub struct WorkflowRunStore {
    db: Arc<Database>,
}

impl WorkflowRunStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Save (upsert) workflow state.
    pub fn save(
        &self,
        run_id: &str,
        session_key: Option<&str>,
        workflow_name: &str,
        input: &str,
        current_step: i32,
        total_steps: i32,
        state_json: &str,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            // Try update first, insert if not found
            let updated = diesel::update(
                workflow_runs::table.filter(workflow_runs::run_id.eq(run_id)),
            )
            .set((
                workflow_runs::current_step.eq(current_step),
                workflow_runs::total_steps.eq(total_steps),
                workflow_runs::state_json.eq(state_json),
                workflow_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;

            if updated == 0 {
                diesel::insert_into(workflow_runs::table)
                    .values(NewWorkflowRun {
                        run_id,
                        session_key,
                        workflow_name,
                        input,
                        status: RunStatus::Running.as_str(),
                        current_step,
                        total_steps,
                        state_json,
                    })
                    .execute(conn)?;
                debug!(run_id, workflow_name, "workflow run created");
            } else {
                debug!(run_id, "workflow run state updated");
            }
            Ok(())
        })
    }

    /// Load workflow state JSON by run_id and workflow_name.
    pub fn load(
        &self,
        run_id: &str,
        workflow_name: &str,
    ) -> PersistenceResult<Option<String>> {
        self.db.with(|conn| {
            let result = workflow_runs::table
                .filter(workflow_runs::run_id.eq(run_id))
                .filter(workflow_runs::workflow_name.eq(workflow_name))
                .select(workflow_runs::state_json)
                .first::<String>(conn)
                .optional()?;
            Ok(result)
        })
    }

    /// List all saved run IDs for a given workflow.
    pub fn list_runs(&self, workflow_name: &str) -> PersistenceResult<Vec<String>> {
        self.db.with(|conn| {
            let ids = workflow_runs::table
                .filter(workflow_runs::workflow_name.eq(workflow_name))
                .select(workflow_runs::run_id)
                .order(workflow_runs::updated_at.desc())
                .load::<String>(conn)?;
            Ok(ids)
        })
    }

    /// Remove a saved workflow run.
    pub fn remove(&self, run_id: &str, workflow_name: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::delete(
                workflow_runs::table
                    .filter(workflow_runs::run_id.eq(run_id))
                    .filter(workflow_runs::workflow_name.eq(workflow_name)),
            )
            .execute(conn)?;
            debug!(run_id, workflow_name, "workflow run removed");
            Ok(())
        })
    }

    /// Mark a workflow run as completed.
    pub fn complete_run(&self, run_id: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                workflow_runs::table.filter(workflow_runs::run_id.eq(run_id)),
            )
            .set((
                workflow_runs::status.eq(RunStatus::Completed.as_str()),
                workflow_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            Ok(())
        })
    }

    /// Mark a workflow run as failed.
    pub fn fail_run(&self, run_id: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(
                workflow_runs::table.filter(workflow_runs::run_id.eq(run_id)),
            )
            .set((
                workflow_runs::status.eq(RunStatus::Failed.as_str()),
                workflow_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            Ok(())
        })
    }

    /// Suspend all running workflow runs (called on startup for crash recovery).
    pub fn suspend_incomplete(&self) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let count = diesel::update(
                workflow_runs::table
                    .filter(workflow_runs::status.eq(RunStatus::Running.as_str())),
            )
            .set((
                workflow_runs::status.eq(RunStatus::Suspended.as_str()),
                workflow_runs::updated_at.eq(db::now_sql()),
            ))
            .execute(conn)?;
            if count > 0 {
                info!(count, "suspended incomplete workflow runs");
            }
            Ok(count)
        })
    }

    /// Find suspended workflow runs for a session.
    pub fn find_suspended(&self, session_key: &str) -> PersistenceResult<Vec<WorkflowRunRow>> {
        self.db.with(|conn| {
            let rows = workflow_runs::table
                .filter(workflow_runs::session_key.eq(Some(session_key)))
                .filter(workflow_runs::status.eq(RunStatus::Suspended.as_str()))
                .order(workflow_runs::updated_at.desc())
                .load::<WorkflowRunRow>(conn)?;
            Ok(rows)
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
    fn test_save_and_load() {
        let db = test_db();
        let store = WorkflowRunStore::new(db);

        store
            .save(
                "run1",
                Some("sess1"),
                "feature-dev",
                "build auth",
                1,
                3,
                r#"{"workflow_name":"feature-dev","input":"build auth"}"#,
            )
            .unwrap();

        let json = store.load("run1", "feature-dev").unwrap().unwrap();
        assert!(json.contains("feature-dev"));

        let runs = store.list_runs("feature-dev").unwrap();
        assert_eq!(runs, vec!["run1"]);
    }

    #[test]
    fn test_remove() {
        let db = test_db();
        let store = WorkflowRunStore::new(db);

        store
            .save("run1", None, "test", "input", 0, 2, "{}")
            .unwrap();
        store.remove("run1", "test").unwrap();

        assert!(store.load("run1", "test").unwrap().is_none());
    }

    #[test]
    fn test_suspend_and_find() {
        let db = test_db();
        let store = WorkflowRunStore::new(db);

        store
            .save("run1", Some("sess1"), "wf1", "i1", 0, 2, "{}")
            .unwrap();
        store
            .save("run2", Some("sess2"), "wf2", "i2", 0, 3, "{}")
            .unwrap();
        store.complete_run("run2").unwrap();

        let suspended = store.suspend_incomplete().unwrap();
        assert_eq!(suspended, 1);

        let runs = store.find_suspended("sess1").unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, "run1");
    }

    #[test]
    fn test_upsert() {
        let db = test_db();
        let store = WorkflowRunStore::new(db);

        store
            .save("run1", None, "test", "input", 0, 2, r#"{"step":0}"#)
            .unwrap();
        store
            .save("run1", None, "test", "input", 1, 2, r#"{"step":1}"#)
            .unwrap();

        let json = store.load("run1", "test").unwrap().unwrap();
        assert!(json.contains("\"step\":1"));

        let runs = store.list_runs("test").unwrap();
        assert_eq!(runs.len(), 1);
    }
}
