use diesel::prelude::*;

use crate::schema::schedules;

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
