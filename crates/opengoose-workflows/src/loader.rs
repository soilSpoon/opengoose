use std::collections::{HashMap, HashSet};
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

    /// Validate referential integrity and detect cycles.
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

        // Validate each agent has either system_prompt or profile
        for agent in &def.agents {
            if agent.system_prompt.is_empty() && agent.profile.is_none() {
                return Err(WorkflowError::InvalidDefinition {
                    reason: format!(
                        "agent '{}' must have either system_prompt or profile",
                        agent.id
                    ),
                });
            }
        }

        let agent_ids: HashSet<&str> = def.agents.iter().map(|a| a.id.as_str()).collect();
        let step_ids: HashSet<&str> = def.steps.iter().map(|s| s.id.as_str()).collect();

        // Check for duplicate step IDs
        if step_ids.len() != def.steps.len() {
            return Err(WorkflowError::InvalidDefinition {
                reason: "duplicate step IDs found".into(),
            });
        }

        for step in &def.steps {
            if !agent_ids.contains(step.agent.as_str()) {
                return Err(WorkflowError::UnknownAgent {
                    step: step.id.clone(),
                    agent: step.agent.clone(),
                });
            }

            for dep in &step.depends_on {
                if !step_ids.contains(dep.as_str()) {
                    return Err(WorkflowError::UnknownDependency {
                        step: step.id.clone(),
                        dependency: dep.clone(),
                    });
                }
                if dep == &step.id {
                    return Err(WorkflowError::CyclicDependency {
                        chain: format!("{} -> {}", step.id, step.id),
                    });
                }
            }

            // Validate loop config references
            if let Some(ref loop_config) = step.loop_config {
                if loop_config.over.is_empty() {
                    return Err(WorkflowError::InvalidDefinition {
                        reason: format!(
                            "step '{}': loop.over cannot be empty",
                            step.id
                        ),
                    });
                }
                if loop_config.verify_each {
                    match &loop_config.verify_step {
                        Some(verify_id) => {
                            if !step_ids.contains(verify_id.as_str()) {
                                return Err(WorkflowError::InvalidDefinition {
                                    reason: format!(
                                        "step '{}': loop.verify_step '{}' references unknown step",
                                        step.id, verify_id
                                    ),
                                });
                            }
                        }
                        None => {
                            return Err(WorkflowError::InvalidDefinition {
                                reason: format!(
                                    "step '{}': loop.verify_each is true but no verify_step specified",
                                    step.id
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Ensure dependencies reference earlier steps (topological order).
        // Since execution is sequential, a step can only depend on steps that
        // appear before it in the list.
        let step_order: HashMap<&str, usize> = def
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        for (i, step) in def.steps.iter().enumerate() {
            for dep in &step.depends_on {
                if let Some(&dep_idx) = step_order.get(dep.as_str()) {
                    if dep_idx >= i {
                        return Err(WorkflowError::InvalidDefinition {
                            reason: format!(
                                "step '{}' depends on '{}' which appears at or after it \
                                 (steps execute sequentially, dependencies must come first)",
                                step.id, dep
                            ),
                        });
                    }
                }
            }
        }

        // DFS cycle detection for transitive cycles
        Self::detect_cycles(def)?;

        Ok(())
    }

    /// Detect circular dependencies using DFS with coloring.
    /// White = unvisited, Gray = in current path, Black = fully explored.
    fn detect_cycles(def: &WorkflowDef) -> Result<(), WorkflowError> {
        let dep_map: HashMap<&str, &[String]> = def
            .steps
            .iter()
            .map(|s| (s.id.as_str(), s.depends_on.as_slice()))
            .collect();

        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut color: HashMap<&str, Color> =
            dep_map.keys().map(|&id| (id, Color::White)).collect();
        let mut path: Vec<&str> = Vec::new();

        fn dfs<'a>(
            node: &'a str,
            dep_map: &HashMap<&'a str, &'a [String]>,
            color: &mut HashMap<&'a str, Color>,
            path: &mut Vec<&'a str>,
        ) -> Result<(), WorkflowError> {
            color.insert(node, Color::Gray);
            path.push(node);

            if let Some(deps) = dep_map.get(node) {
                for dep in *deps {
                    let dep = dep.as_str();
                    match color.get(dep) {
                        Some(Color::Gray) => {
                            let cycle_start =
                                path.iter().position(|&n| n == dep).unwrap();
                            let mut chain: Vec<&str> = path[cycle_start..].to_vec();
                            chain.push(dep);
                            return Err(WorkflowError::CyclicDependency {
                                chain: chain.join(" -> "),
                            });
                        }
                        Some(Color::White) | None => {
                            dfs(dep, dep_map, color, path)?;
                        }
                        Some(Color::Black) => {}
                    }
                }
            }

            path.pop();
            color.insert(node, Color::Black);
            Ok(())
        }

        let ids: Vec<&str> = dep_map.keys().copied().collect();
        for id in ids {
            if color[id] == Color::White {
                dfs(id, &dep_map, &mut color, &mut path)?;
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
