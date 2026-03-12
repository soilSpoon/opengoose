use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct WorkflowItem {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub workflow: String,
    pub agent_count: usize,
    pub schedule_count: usize,
    pub enabled_schedule_count: usize,
    pub trigger_count: usize,
    pub enabled_trigger_count: usize,
    pub last_run_status: Option<String>,
    pub last_run_at: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowStep {
    pub profile: String,
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowAutomation {
    pub kind: &'static str,
    pub name: String,
    pub enabled: bool,
    pub detail: String,
    pub note: String,
}

#[derive(Serialize)]
pub struct WorkflowRun {
    pub team_run_id: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct WorkflowDetail {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub workflow: String,
    pub source_label: String,
    pub yaml: String,
    pub steps: Vec<WorkflowStep>,
    pub automations: Vec<WorkflowAutomation>,
    pub recent_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize, Default)]
pub struct TriggerWorkflowRequest {
    pub input: Option<String>,
}

#[derive(Serialize)]
pub struct TriggerWorkflowResponse {
    pub workflow: String,
    pub accepted: bool,
    pub input: String,
}
