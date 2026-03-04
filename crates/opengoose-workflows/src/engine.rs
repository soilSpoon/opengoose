use tracing::{info, warn};

use crate::definition::WorkflowDef;
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
    pub fn current_prompt(&self) -> Option<String> {
        let step_idx = self.state.current_step;
        let step_def = self.definition.steps.get(step_idx)?;

        let mut prompt = step_def.prompt.clone();

        // Substitute {{input}} with the original user request
        prompt = prompt.replace("{{input}}", &self.state.input);

        // Substitute {{step_id}} placeholders with outputs from dependencies
        for dep_id in &step_def.depends_on {
            let placeholder = format!("{{{{{dep_id}}}}}");
            if let Some(output) = self.state.step_output(dep_id) {
                prompt = prompt.replace(&placeholder, output);
            }
        }

        Some(prompt)
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

        let max_retries = self
            .definition
            .steps
            .get(idx)
            .map(|d| d.max_retries)
            .unwrap_or(2);

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
                    step.status = StepStatus::Failed;
                } else {
                    info!(step = %step.step_id, retries = step.retries, %reason, "retrying step");
                    step.status = StepStatus::Pending;
                }
            }
            StepOutcome::Failed { reason } => {
                warn!(step = %step.step_id, %reason, "step failed permanently");
                step.status = StepStatus::Failed;
            }
        }

        !self.state.is_complete() && !self.state.is_failed()
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
            .filter(|s| s.status == StepStatus::Completed)
            .count();
        (completed, self.total_steps())
    }
}
