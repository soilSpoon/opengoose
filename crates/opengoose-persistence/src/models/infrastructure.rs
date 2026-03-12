use diesel::prelude::*;

use crate::schema::*;

// ── Schedules ──

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

// ── Plugins ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = plugins)]
pub struct PluginRow {
    pub id: i32,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub capabilities: String,
    pub source_path: String,
    pub enabled: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = plugins)]
pub struct NewPlugin<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub author: Option<&'a str>,
    pub description: Option<&'a str>,
    pub capabilities: &'a str,
    pub source_path: &'a str,
}

// ── Triggers ──

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

// ── API Keys ──

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = api_keys)]
pub struct ApiKeyRow {
    pub id: String,
    pub key_hash: String,
    pub description: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = api_keys)]
pub struct NewApiKey<'a> {
    pub id: &'a str,
    pub key_hash: &'a str,
    pub description: Option<&'a str>,
}
