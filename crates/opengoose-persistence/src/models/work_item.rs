use diesel::prelude::*;

use crate::schema::{work_item_compacted, work_items};

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
