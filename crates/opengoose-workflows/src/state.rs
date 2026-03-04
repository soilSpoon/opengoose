use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Runtime state of a workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Which workflow definition is running.
    pub workflow_name: String,

    /// Original user input that triggered the workflow.
    pub input: String,

    /// Per-step state, in execution order.
    pub steps: Vec<StepState>,

    /// Index of the current step (0-based).
    pub current_step: usize,

    /// Shared mutable context accumulated across steps.
    /// Populated by parsing `KEY: value` lines from step outputs (antfarm-style).
    /// All `{{key}}` placeholders in prompts are resolved against this map
    /// in addition to `{{input}}` and `{{step_id}}` references.
    pub context: HashMap<String, String>,
}

impl WorkflowState {
    pub fn new(workflow_name: String, input: String, step_ids: Vec<String>) -> Self {
        let steps = step_ids
            .into_iter()
            .map(|id| StepState {
                step_id: id,
                status: StepStatus::Pending,
                output: None,
                retries: 0,
                loop_state: None,
            })
            .collect();

        Self {
            workflow_name,
            input,
            steps,
            current_step: 0,
            context: HashMap::new(),
        }
    }

    /// Get the output of a completed step by ID.
    pub fn step_output(&self, step_id: &str) -> Option<&str> {
        self.steps
            .iter()
            .find(|s| s.step_id == step_id)
            .and_then(|s| s.output.as_deref())
    }

    /// Whether all steps have completed (or been skipped).
    pub fn is_complete(&self) -> bool {
        self.steps
            .iter()
            .all(|s| matches!(s.status, StepStatus::Completed | StepStatus::Skipped))
    }

    /// Whether any step has permanently failed.
    pub fn is_failed(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Failed)
    }

    /// Whether the workflow has reached a terminal state (complete or failed).
    pub fn is_terminal(&self) -> bool {
        self.is_complete() || self.is_failed()
    }

    /// Parse `KEY: value` lines from output text and merge into context.
    /// Only parses lines matching `^UPPER_SNAKE_CASE: ...` pattern
    /// (antfarm convention for structured output).
    pub fn extract_context(&mut self, output: &str) {
        for line in output.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                // Only accept UPPER_SNAKE_CASE keys (like antfarm)
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                {
                    self.context
                        .insert(key.to_lowercase(), value.trim().to_string());
                }
            }
        }
    }
}

/// State of an individual step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub step_id: String,
    pub status: StepStatus,
    pub output: Option<String>,
    pub retries: u32,
    /// Loop iteration state (only present for loop steps).
    pub loop_state: Option<LoopState>,
}

/// Tracks iteration progress for a loop step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    /// All items to iterate over (parsed from a prior step's STORIES_JSON output).
    pub items: Vec<String>,
    /// Index of the current item being processed.
    pub current_index: usize,
    /// Outputs collected per iteration.
    pub iteration_outputs: Vec<Option<String>>,
}

impl LoopState {
    pub fn new(items: Vec<String>) -> Self {
        let len = items.len();
        Self {
            items,
            current_index: 0,
            iteration_outputs: vec![None; len],
        }
    }

    pub fn current_item(&self) -> Option<&str> {
        self.items.get(self.current_index).map(|s| s.as_str())
    }

    pub fn is_done(&self) -> bool {
        self.current_index >= self.items.len()
    }

    pub fn advance(&mut self) {
        self.current_index += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Skipped,
    Failed,
}
