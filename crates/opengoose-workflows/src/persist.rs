use std::path::PathBuf;

use tracing::info;

use crate::error::WorkflowError;
use crate::state::{WorkflowState, STATE_SCHEMA_VERSION};

/// JSON file-based persistence for workflow state.
///
/// Saves state after each step completes so workflows can be resumed
/// after crashes. Each run is stored as a separate JSON file named
/// `{workflow_name}_{run_id}.json` in the configured directory.
pub struct WorkflowStore {
    dir: PathBuf,
}

impl WorkflowStore {
    /// Create a store that saves workflow state to the given directory.
    pub fn new(dir: PathBuf) -> Result<Self, WorkflowError> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Default directory: `~/.opengoose/workflows/`
    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".opengoose")
            .join("workflows")
    }

    /// Save workflow state to a JSON file.
    pub fn save(&self, run_id: &str, state: &WorkflowState) -> Result<PathBuf, WorkflowError> {
        let filename = format!("{}_{}.json", state.workflow_name, run_id);
        let path = self.dir.join(&filename);

        let json = serde_json::to_string_pretty(state).map_err(|e| {
            WorkflowError::InvalidDefinition {
                reason: format!("failed to serialize state: {e}"),
            }
        })?;

        std::fs::write(&path, json)?;
        info!(path = %path.display(), "saved workflow state");
        Ok(path)
    }

    /// Load workflow state from a JSON file.
    pub fn load(&self, run_id: &str, workflow_name: &str) -> Result<WorkflowState, WorkflowError> {
        let filename = format!("{workflow_name}_{run_id}.json");
        let path = self.dir.join(&filename);

        let json = std::fs::read_to_string(&path)?;
        let state: WorkflowState = serde_json::from_str(&json).map_err(|e| {
            WorkflowError::InvalidDefinition {
                reason: format!("failed to deserialize state: {e}"),
            }
        })?;

        if state.schema_version != STATE_SCHEMA_VERSION {
            return Err(WorkflowError::InvalidDefinition {
                reason: format!(
                    "saved state has schema version {} but current is {}; \
                     delete the file and re-run the workflow",
                    state.schema_version, STATE_SCHEMA_VERSION
                ),
            });
        }

        info!(path = %path.display(), "loaded workflow state");
        Ok(state)
    }

    /// List all saved runs for a given workflow.
    pub fn list_runs(&self, workflow_name: &str) -> Result<Vec<String>, WorkflowError> {
        let prefix = format!("{workflow_name}_");
        let mut runs = Vec::new();

        if !self.dir.is_dir() {
            return Ok(runs);
        }

        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                let run_id = name
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                    .unwrap_or("")
                    .to_string();
                if !run_id.is_empty() {
                    runs.push(run_id);
                }
            }
        }

        Ok(runs)
    }

    /// Remove a saved run.
    pub fn remove(&self, run_id: &str, workflow_name: &str) -> Result<(), WorkflowError> {
        let filename = format!("{workflow_name}_{run_id}.json");
        let path = self.dir.join(&filename);

        if path.exists() {
            std::fs::remove_file(&path)?;
            info!(path = %path.display(), "removed workflow state");
        }
        Ok(())
    }
}
