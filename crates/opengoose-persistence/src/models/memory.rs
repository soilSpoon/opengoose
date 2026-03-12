use diesel::prelude::*;

use crate::schema::agent_memories;

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
