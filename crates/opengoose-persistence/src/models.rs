use diesel::prelude::*;

use crate::schema::*;

// ── Sessions ──

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub session_key: &'a str,
}

// ── Messages ──

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub session_key: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub author: Option<&'a str>,
}

// ── Message Queue ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = message_queue)]
pub struct QueueMessageRow {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub sender: String,
    pub recipient: String,
    pub content: String,
    pub msg_type: String,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = message_queue)]
pub struct NewQueueMessage<'a> {
    pub session_key: &'a str,
    pub team_run_id: &'a str,
    pub sender: &'a str,
    pub recipient: &'a str,
    pub content: &'a str,
    pub msg_type: &'a str,
}

// ── Work Items ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = work_items)]
pub struct WorkItemRow {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_to: Option<String>,
    pub workflow_step: Option<i32>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = work_items)]
pub struct NewWorkItem<'a> {
    pub session_key: &'a str,
    pub team_run_id: &'a str,
    pub parent_id: Option<i32>,
    pub title: &'a str,
}

// ── Orchestration Runs ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = orchestration_runs)]
pub struct OrchestrationRunRow {
    #[allow(dead_code)]
    pub id: i32,
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub input: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = orchestration_runs)]
pub struct NewOrchestrationRun<'a> {
    pub team_run_id: &'a str,
    pub session_key: &'a str,
    pub team_name: &'a str,
    pub workflow: &'a str,
    pub input: &'a str,
    pub total_steps: i32,
}

// ── Workflow Runs ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = workflow_runs)]
pub struct WorkflowRunRow {
    #[allow(dead_code)]
    pub id: i32,
    pub run_id: String,
    pub session_key: Option<String>,
    pub workflow_name: String,
    pub input: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub state_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = workflow_runs)]
pub struct NewWorkflowRun<'a> {
    pub run_id: &'a str,
    pub session_key: Option<&'a str>,
    pub workflow_name: &'a str,
    pub input: &'a str,
    pub status: &'a str,
    pub current_step: i32,
    pub total_steps: i32,
    pub state_json: &'a str,
}
