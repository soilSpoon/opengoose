use diesel::prelude::*;

use crate::schema::triggers;

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
