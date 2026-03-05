use std::sync::Arc;

use tracing::info;

use opengoose_persistence::{Database, WorkflowRunStore};

use crate::error::WorkflowError;
use crate::state::{WorkflowState, STATE_SCHEMA_VERSION};

/// Database-backed persistence for workflow state.
///
/// Saves state after each step completes so workflows can be resumed
/// after crashes. Uses the shared SQLite database via `WorkflowRunStore`.
pub struct WorkflowStore {
    inner: WorkflowRunStore,
}

impl WorkflowStore {
    /// Create a store backed by the shared database.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            inner: WorkflowRunStore::new(db),
        }
    }

    /// Save workflow state to the database.
    pub fn save(
        &self,
        run_id: &str,
        session_key: Option<&str>,
        state: &WorkflowState,
    ) -> Result<(), WorkflowError> {
        let json = serde_json::to_string(state).map_err(|e| {
            WorkflowError::InvalidDefinition {
                reason: format!("failed to serialize state: {e}"),
            }
        })?;

        let (completed, total) = (state.current_step as i32, state.steps.len() as i32);

        self.inner
            .save(
                run_id,
                session_key,
                &state.workflow_name,
                &state.input,
                completed,
                total,
                &json,
            )
            .map_err(|e| WorkflowError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        info!(run_id, workflow = %state.workflow_name, "saved workflow state");
        Ok(())
    }

    /// Load workflow state from the database.
    pub fn load(&self, run_id: &str, workflow_name: &str) -> Result<WorkflowState, WorkflowError> {
        let json = self
            .inner
            .load(run_id, workflow_name)
            .map_err(|e| WorkflowError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .ok_or_else(|| WorkflowError::NotFound {
                name: format!("workflow run '{run_id}' for '{workflow_name}'"),
            })?;

        let state: WorkflowState = serde_json::from_str(&json).map_err(|e| {
            WorkflowError::InvalidDefinition {
                reason: format!("failed to deserialize state: {e}"),
            }
        })?;

        if state.schema_version != STATE_SCHEMA_VERSION {
            return Err(WorkflowError::InvalidDefinition {
                reason: format!(
                    "saved state has schema version {} but current is {}; \
                     remove the run and re-execute the workflow",
                    state.schema_version, STATE_SCHEMA_VERSION
                ),
            });
        }

        Ok(state)
    }

    /// List all saved run IDs for a given workflow.
    pub fn list_runs(&self, workflow_name: &str) -> Result<Vec<String>, WorkflowError> {
        self.inner
            .list_runs(workflow_name)
            .map_err(|e| WorkflowError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }

    /// Remove a saved workflow run.
    pub fn remove(&self, run_id: &str, workflow_name: &str) -> Result<(), WorkflowError> {
        self.inner
            .remove(run_id, workflow_name)
            .map_err(|e| WorkflowError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }
}
