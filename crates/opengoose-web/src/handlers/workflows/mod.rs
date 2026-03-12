/// JSON API handlers for workflow definitions and manual triggers.
mod execution;
mod history;
mod listing;
mod models;

pub use execution::trigger_workflow;
pub use listing::{get_workflow, list_workflows};
pub use models::{
    TriggerWorkflowRequest, TriggerWorkflowResponse, WorkflowAutomation, WorkflowDetail,
    WorkflowItem, WorkflowRun, WorkflowStep,
};

#[cfg(test)]
mod tests;
