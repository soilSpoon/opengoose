use diesel::prelude::*;

use crate::schema::*;

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
    pub hash_id: Option<String>,
    pub is_ephemeral: i32,
    pub priority: i32,
}

#[derive(Insertable)]
#[diesel(table_name = work_items)]
pub struct NewWorkItem<'a> {
    pub session_key: &'a str,
    pub team_run_id: &'a str,
    pub parent_id: Option<i32>,
    pub title: &'a str,
    pub hash_id: Option<&'a str>,
    pub is_ephemeral: i32,
    pub priority: i32,
}

// ── Work Item Relations ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = work_item_relations)]
pub struct RelationRow {
    pub id: i32,
    pub from_item_id: i32,
    pub to_item_id: i32,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = work_item_relations)]
pub struct NewRelation<'a> {
    pub from_item_id: i32,
    pub to_item_id: i32,
    pub relation_type: &'a str,
}

// ── Work Item Compacted ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = work_item_compacted)]
pub struct CompactedRow {
    pub id: i32,
    pub team_run_id: String,
    pub parent_id: Option<i32>,
    pub summary: String,
    pub item_count: i32,
    pub created_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = work_item_compacted)]
pub struct NewCompacted<'a> {
    pub team_run_id: &'a str,
    pub parent_id: Option<i32>,
    pub summary: &'a str,
    pub item_count: i32,
}

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
