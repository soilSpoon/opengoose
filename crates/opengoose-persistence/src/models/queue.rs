use diesel::prelude::*;

use crate::schema::message_queue;

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
