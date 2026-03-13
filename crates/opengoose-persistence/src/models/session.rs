use diesel::prelude::*;

use crate::schema::*;

// ── Sessions ──

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub session_key: &'a str,
    pub selected_model: Option<&'a str>,
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

// ── Agent Messages ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = agent_messages)]
pub struct AgentMessageRow {
    pub id: i32,
    pub session_key: String,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub channel: Option<String>,
    pub payload: String,
    pub status: String,
    pub created_at: String,
    pub delivered_at: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = agent_messages)]
pub struct NewAgentMessage<'a> {
    pub session_key: &'a str,
    pub from_agent: &'a str,
    pub to_agent: Option<&'a str>,
    pub channel: Option<&'a str>,
    pub payload: &'a str,
}
