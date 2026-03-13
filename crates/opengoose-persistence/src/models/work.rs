use diesel::prelude::*;

use crate::schema::*;

// ── Agent Memories ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = agent_memories)]
pub struct AgentMemoryRow {
    pub id: i32,
    pub agent_name: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = agent_memories)]
pub struct NewAgentMemory<'a> {
    pub agent_name: &'a str,
    pub key: &'a str,
    pub value: &'a str,
}

// ── Orchestration Runs ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = orchestration_runs)]
pub struct OrchestrationRunRow {
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
