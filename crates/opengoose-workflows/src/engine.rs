use tracing::{info, warn};

use crate::definition::{OnFailStrategy, WorkflowDef};
use crate::error::WorkflowError;
use crate::state::{StepStatus, WorkflowState};

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

/// Structured context passed to the execution callback, replacing the
/// ambiguous `(String, String)` tuple from v1.
#[derive(Debug, Clone)]
pub struct StepContext {
    pub step_id: String,
    pub step_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub progress: (usize, usize),
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

    /// Get current workflow state (for persistence or UI display).
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Build the fully-resolved prompt for the current step, injecting
    /// context from dependencies and the original user input.
    ///
    /// Returns `Err` if any dependency lacks output (instead of silently
    /// leaving `{{placeholder}}` in the prompt).
    pub fn current_prompt(&self) -> Result<Option<String>, WorkflowError> {
        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return Ok(None),
        };

        let mut prompt = step_def.prompt.clone();

        // Substitute {{input}} with the original user request
        prompt = prompt.replace("{{input}}", &self.state.input);

        // Substitute {{step_id}} placeholders with outputs from dependencies
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

        // Append acceptance criteria so the agent knows what success looks like
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

        let step_def = &self.definition.steps[self.state.current_step];
        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)
            .expect("validated at load time");

        Ok(Some(StepContext {
            step_id: step_def.id.clone(),
            step_name: step_def.name.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            system_prompt: agent.system_prompt.clone(),
            user_prompt: prompt,
            progress: self.progress(),
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

    /// Record the outcome of executing the current step and advance.
    ///
    /// Returns `true` if the workflow has more steps to execute.
    pub fn record_outcome(&mut self, outcome: StepOutcome) -> bool {
        let idx = self.state.current_step;
        let step = match self.state.steps.get_mut(idx) {
            Some(s) => s,
            None => return false,
        };

        let step_def = match self.definition.steps.get(idx) {
            Some(d) => d,
            None => return false,
        };
        let max_retries = step_def.max_retries;
        let on_fail = &step_def.on_fail;

        match outcome {
            StepOutcome::Completed { output } => {
                info!(step = %step.step_id, "step completed");
                step.status = StepStatus::Completed;
                step.output = Some(output);
                self.state.current_step += 1;
            }
            StepOutcome::Retry { reason } => {
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
