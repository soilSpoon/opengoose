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
            })
            .collect();

        Self {
            workflow_name,
            input,
            steps,
            current_step: 0,
        }
    }

    /// Get the output of a completed step by ID.
    pub fn step_output(&self, step_id: &str) -> Option<&str> {
        self.steps
            .iter()
            .find(|s| s.step_id == step_id)
            .and_then(|s| s.output.as_deref())
    }

    /// Whether all steps have completed.
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.status == StepStatus::Completed)
    }

    /// Whether any step has permanently failed.
    pub fn is_failed(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Failed)
    }
}

/// State of an individual step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub step_id: String,
    pub status: StepStatus,
    pub output: Option<String>,
    pub retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}
