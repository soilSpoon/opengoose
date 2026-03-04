use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::info;

use crate::definition::WorkflowDef;
use crate::error::WorkflowError;

/// Loads and validates workflow definitions from YAML files.
pub struct WorkflowLoader {
    workflows: HashMap<String, WorkflowDef>,
}

impl WorkflowLoader {
    /// Create an empty loader.
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
        }
    }

    /// Load all `.yaml` / `.yml` files from a directory.
    pub fn load_dir(&mut self, dir: &Path) -> Result<usize, WorkflowError> {
        let mut count = 0;

        if !dir.is_dir() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            let is_yaml = path
                .extension()
                .map(|ext| ext == "yaml" || ext == "yml")
                .unwrap_or(false);

            if is_yaml {
                self.load_file(&path)?;
                count += 1;
            }
        }

        info!(count, dir = %dir.display(), "loaded workflow definitions");
        Ok(count)
    }

    /// Load a single YAML workflow definition.
    pub fn load_file(&mut self, path: &Path) -> Result<(), WorkflowError> {
        let content = std::fs::read_to_string(path)?;
        let def: WorkflowDef = serde_yaml::from_str(&content)?;

        Self::validate(&def)?;

        info!(name = %def.name, steps = def.steps.len(), agents = def.agents.len(), "loaded workflow");
        self.workflows.insert(def.name.clone(), def);
        Ok(())
    }

    /// Load a workflow definition from a YAML string.
    pub fn load_str(&mut self, yaml: &str) -> Result<(), WorkflowError> {
        let def: WorkflowDef = serde_yaml::from_str(yaml)?;
        Self::validate(&def)?;
        self.workflows.insert(def.name.clone(), def);
        Ok(())
    }

    /// Get a loaded workflow by name.
    pub fn get(&self, name: &str) -> Option<&WorkflowDef> {
        self.workflows.get(name)
    }

    /// List all loaded workflow names.
    pub fn list(&self) -> Vec<&str> {
        self.workflows.keys().map(|s| s.as_str()).collect()
    }

    /// Default directory for bundled workflows.
    pub fn bundled_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("workflows")
    }

    /// Validate referential integrity of a workflow definition.
    fn validate(def: &WorkflowDef) -> Result<(), WorkflowError> {
        if def.name.is_empty() {
            return Err(WorkflowError::InvalidDefinition {
                reason: "workflow name cannot be empty".into(),
            });
        }

        if def.steps.is_empty() {
            return Err(WorkflowError::InvalidDefinition {
                reason: "workflow must have at least one step".into(),
            });
        }

        if def.agents.is_empty() {
            return Err(WorkflowError::InvalidDefinition {
                reason: "workflow must define at least one agent".into(),
            });
        }

        let agent_ids: Vec<&str> = def.agents.iter().map(|a| a.id.as_str()).collect();
        let step_ids: Vec<&str> = def.steps.iter().map(|s| s.id.as_str()).collect();

        for step in &def.steps {
            if !agent_ids.contains(&step.agent.as_str()) {
                return Err(WorkflowError::UnknownAgent {
                    step: step.id.clone(),
                    agent: step.agent.clone(),
                });
            }

            for dep in &step.depends_on {
                if !step_ids.contains(&dep.as_str()) {
                    return Err(WorkflowError::UnknownDependency {
                        step: step.id.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

impl Default for WorkflowLoader {
    fn default() -> Self {
        Self::new()
    }
}
