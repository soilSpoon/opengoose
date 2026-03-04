use tracing::{info, warn};

use crate::definition::{OnFailStrategy, WorkflowDef};
use crate::error::WorkflowError;
use crate::state::{LoopState, StepStatus, WorkflowState};

/// Outcome of executing a single step.
#[derive(Debug)]
pub enum StepOutcome {
    /// Step completed successfully with output text.
    Completed { output: String },
    /// Step needs to be retried (criteria not met).
    Retry { reason: String },
    /// Step permanently failed.
    Failed { reason: String },
}

/// Structured context passed to the execution callback.
#[derive(Debug, Clone)]
pub struct StepContext {
    pub step_id: String,
    pub step_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub progress: (usize, usize),
    /// For loop steps: which iteration (0-based) and total items.
    pub loop_iteration: Option<(usize, usize)>,
}

/// Drives a workflow through its steps.
///
/// The engine itself is transport-agnostic: callers provide an `execute_fn`
/// callback that sends the prompt to whichever LLM backend they use (Goose
/// sessions, direct API calls, etc.).
pub struct WorkflowEngine {
    definition: WorkflowDef,
    state: WorkflowState,
}

impl WorkflowEngine {
    /// Create a new engine for the given workflow definition and user input.
    pub fn new(definition: WorkflowDef, input: String) -> Self {
        let step_ids: Vec<String> = definition.steps.iter().map(|s| s.id.clone()).collect();
        let state = WorkflowState::new(definition.name.clone(), input, step_ids);
        Self { definition, state }
    }

    /// Resume from a previously persisted state.
    pub fn resume(definition: WorkflowDef, state: WorkflowState) -> Self {
        Self { definition, state }
    }

    /// Get current workflow state (for persistence or UI display).
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Get a mutable reference to the workflow state.
    pub fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }

    /// Build the fully-resolved prompt for the current step.
    ///
    /// Resolves placeholders in this order:
    /// 1. `{{input}}` — original user request
    /// 2. `{{step_id}}` — output from a named dependency step
    /// 3. `{{key}}` — value from the shared mutable context
    /// 4. Loop-specific: `{{current_item}}`, `{{completed_items}}`, `{{items_remaining}}`
    ///
    /// Returns `Err` if a declared dependency has no output.
    pub fn current_prompt(&self) -> Result<Option<String>, WorkflowError> {
        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return Ok(None),
        };

        let mut prompt = step_def.prompt.clone();

        // 1. Substitute {{input}}
        prompt = prompt.replace("{{input}}", &self.state.input);

        // 2. Substitute {{step_id}} from dependency outputs
        for dep_id in &step_def.depends_on {
            let placeholder = format!("{{{{{dep_id}}}}}");
            match self.state.step_output(dep_id) {
                Some(output) => {
                    prompt = prompt.replace(&placeholder, output);
                }
                None => {
                    let dep_status = self
                        .state
                        .steps
                        .iter()
                        .find(|s| s.step_id == *dep_id)
                        .map(|s| format!("{:?}", s.status))
                        .unwrap_or_else(|| "unknown".into());
                    return Err(WorkflowError::UnsatisfiedDependency {
                        step: step_def.id.clone(),
                        dependency: dep_id.clone(),
                        status: dep_status,
                    });
                }
            }
        }

        // 3. Substitute {{key}} from shared context (antfarm-style)
        for (key, value) in &self.state.context {
            let placeholder = format!("{{{{{key}}}}}");
            prompt = prompt.replace(&placeholder, value);
        }

        // 4. Loop-specific substitutions
        if let Some(loop_state) = self
            .state
            .steps
            .get(step_idx)
            .and_then(|s| s.loop_state.as_ref())
        {
            if let Some(item) = loop_state.current_item() {
                prompt = prompt.replace("{{current_item}}", item);
            }

            let completed: Vec<&str> = loop_state
                .iteration_outputs
                .iter()
                .filter_map(|o| o.as_deref())
                .collect();
            prompt = prompt.replace("{{completed_items}}", &completed.join("\n---\n"));

            let remaining = loop_state.items.len().saturating_sub(loop_state.current_index + 1);
            prompt = prompt.replace("{{items_remaining}}", &remaining.to_string());
        }

        // 5. Append acceptance criteria
        if !step_def.expects.is_empty() {
            prompt.push_str("\n\n---\nAcceptance Criteria (your output MUST satisfy all of these):\n");
            for (i, criterion) in step_def.expects.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, criterion));
            }
        }

        Ok(Some(prompt))
    }

    /// Build a structured `StepContext` for the current step.
    pub fn current_step_context(&self) -> Result<Option<StepContext>, WorkflowError> {
        let prompt = match self.current_prompt()? {
            Some(p) => p,
            None => return Ok(None),
        };

        let step_idx = self.state.current_step;
        let step_def = &self.definition.steps[step_idx];
        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)
            .expect("validated at load time");

        let loop_iteration = self.state.steps[step_idx]
            .loop_state
            .as_ref()
            .map(|ls| (ls.current_index, ls.items.len()));

        Ok(Some(StepContext {
            step_id: step_def.id.clone(),
            step_name: step_def.name.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            system_prompt: agent.system_prompt.clone(),
            user_prompt: prompt,
            progress: self.progress(),
            loop_iteration,
        }))
    }

    /// Get the system prompt (persona) for the current step's agent.
    pub fn current_agent_system_prompt(&self) -> Option<&str> {
        let step_def = self.definition.steps.get(self.state.current_step)?;
        self.definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)
            .map(|a| a.system_prompt.as_str())
    }

    /// Get metadata about the current step.
    pub fn current_step_info(&self) -> Option<(&str, &str, &str)> {
        let step_def = self.definition.steps.get(self.state.current_step)?;
        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)?;
        Some((&step_def.id, &step_def.name, &agent.name))
    }

    /// Initialize a loop step by parsing items from context or a dependency.
    ///
    /// Call this before the first iteration of a loop step. The items are
    /// sourced from the context key specified in `loop.over`, parsed as a
    /// JSON array of strings.
    pub fn init_loop(&mut self) -> Result<bool, WorkflowError> {
        let idx = self.state.current_step;
        let step_def = match self.definition.steps.get(idx) {
            Some(s) => s,
            None => return Ok(false),
        };

        let loop_config = match &step_def.loop_config {
            Some(c) => c,
            None => return Ok(false), // Not a loop step
        };

        // Already initialized
        if self.state.steps[idx].loop_state.is_some() {
            return Ok(true);
        }

        let key = &loop_config.over;

        // Try to find items from context (lowercased key)
        let items_json = self
            .state
            .context
            .get(key)
            .cloned()
            .ok_or_else(|| WorkflowError::UnsatisfiedDependency {
                step: step_def.id.clone(),
                dependency: format!("context key '{key}'"),
                status: "not found in context".into(),
            })?;

        // Parse as JSON array of strings (or objects serialized to strings)
        let items: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&items_json)
            .map_err(|e| WorkflowError::InvalidDefinition {
                reason: format!(
                    "loop step '{}': failed to parse '{}' as JSON array: {}",
                    step_def.id, key, e
                ),
            })?
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect();

        if items.is_empty() {
            info!(step = %step_def.id, "loop has zero items, skipping");
            self.state.steps[idx].status = StepStatus::Skipped;
            self.state.current_step += 1;
            return Ok(false);
        }

        info!(step = %step_def.id, items = items.len(), "initialized loop step");
        self.state.steps[idx].loop_state = Some(LoopState::new(items));
        Ok(true)
    }

    /// Record the outcome of executing the current step and advance.
    ///
    /// Returns `true` if the workflow has more steps to execute.
    pub fn record_outcome(&mut self, outcome: StepOutcome) -> bool {
        let idx = self.state.current_step;

        if idx >= self.state.steps.len() || idx >= self.definition.steps.len() {
            return false;
        }

        let max_retries = self.definition.steps[idx].max_retries;
        let on_fail = self.definition.steps[idx].on_fail.clone();
        let is_loop = self.definition.steps[idx].loop_config.is_some();

        match outcome {
            StepOutcome::Completed { output } => {
                // Extract KEY: VALUE pairs into shared context first
                // (before borrowing steps mutably)
                self.state.extract_context(&output);

                let step = &mut self.state.steps[idx];
                if is_loop {
                    if let Some(ref mut ls) = step.loop_state {
                        let cur = ls.current_index;
                        if cur < ls.iteration_outputs.len() {
                            ls.iteration_outputs[cur] = Some(output.clone());
                        }
                        ls.advance();
                        if ls.is_done() {
                            info!(step = %step.step_id, "loop step completed all iterations");
                            step.status = StepStatus::Completed;
                            step.output = Some(output);
                            self.state.current_step += 1;
                        } else {
                            info!(
                                step = %step.step_id,
                                iteration = ls.current_index,
                                total = ls.items.len(),
                                "loop step advancing to next iteration"
                            );
                            step.status = StepStatus::Pending;
                        }
                    } else {
                        step.status = StepStatus::Completed;
                        step.output = Some(output);
                        self.state.current_step += 1;
                    }
                } else {
                    info!(step = %step.step_id, "step completed");
                    step.status = StepStatus::Completed;
                    step.output = Some(output);
                    self.state.current_step += 1;
                }
            }
            StepOutcome::Retry { reason } => {
                let step = &mut self.state.steps[idx];
                step.retries += 1;
                if step.retries >= max_retries {
                    warn!(step = %step.step_id, retries = step.retries, %reason, "step exhausted retries");
                    match on_fail {
                        OnFailStrategy::Skip => {
                            info!(step = %step.step_id, "skipping failed step per on_fail policy");
                            step.status = StepStatus::Skipped;
                            self.state.current_step += 1;
                        }
                        OnFailStrategy::Abort => {
                            step.status = StepStatus::Failed;
                        }
                    }
                } else {
                    info!(step = %step.step_id, retries = step.retries, %reason, "retrying step");
                    step.status = StepStatus::Pending;
                }
            }
            StepOutcome::Failed { reason } => {
                let step = &mut self.state.steps[idx];
                warn!(step = %step.step_id, %reason, "step failed permanently");
                match on_fail {
                    OnFailStrategy::Skip => {
                        info!(step = %step.step_id, "skipping failed step per on_fail policy");
                        step.status = StepStatus::Skipped;
                        self.state.current_step += 1;
                    }
                    OnFailStrategy::Abort => {
                        step.status = StepStatus::Failed;
                    }
                }
            }
        }

        !self.state.is_terminal()
    }

    /// Mark the current step as running.
    pub fn mark_running(&mut self) {
        if let Some(step) = self.state.steps.get_mut(self.state.current_step) {
            step.status = StepStatus::Running;
        }
    }

    /// Whether the current step is a loop step.
    pub fn is_current_loop(&self) -> bool {
        self.definition
            .steps
            .get(self.state.current_step)
            .and_then(|s| s.loop_config.as_ref())
            .is_some()
    }

    /// Total number of steps.
    pub fn total_steps(&self) -> usize {
        self.definition.steps.len()
    }

    /// Summary of progress: (completed, total).
    pub fn progress(&self) -> (usize, usize) {
        let completed = self
            .state
            .steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Completed | StepStatus::Skipped))
            .count();
        (completed, self.total_steps())
    }
}
