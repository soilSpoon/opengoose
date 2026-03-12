use diesel::prelude::*;

use crate::schema::agent_messages;

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
