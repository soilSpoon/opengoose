use diesel::prelude::*;

use crate::schema::{agent_messages, schedules, triggers};

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

#[derive(Queryable, Selectable)]
#[diesel(table_name = schedules)]
pub struct ScheduleRow {
    pub id: i32,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: i32,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = schedules)]
pub struct NewSchedule<'a> {
    pub name: &'a str,
    pub cron_expression: &'a str,
    pub team_name: &'a str,
    pub input: &'a str,
    pub next_run_at: Option<&'a str>,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = triggers)]
pub struct TriggerRow {
    pub id: i32,
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
    pub enabled: i32,
    pub last_fired_at: Option<String>,
    pub fire_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = triggers)]
pub struct NewTrigger<'a> {
    pub name: &'a str,
    pub trigger_type: &'a str,
    pub condition_json: &'a str,
    pub team_name: &'a str,
    pub input: &'a str,
}
