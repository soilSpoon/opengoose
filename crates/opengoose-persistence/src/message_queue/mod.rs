mod storage;
#[cfg(test)]
mod tests;

use crate::db::Database;
use crate::db_enum::db_enum;
use crate::error::PersistenceError;
use crate::models::QueueMessageRow;
use std::sync::Arc;

db_enum! {
    /// Status of a queued message.
    pub enum MessageStatus {
        Pending => "pending",
        Processing => "processing",
        Completed => "completed",
        Failed => "failed",
        Dead => "dead",
    }
}

db_enum! {
    /// Type of a queued message.
    pub enum MessageType {
        /// A task to be executed by an agent.
        Task => "task",
        /// A result returned by an agent.
        Result => "result",
        /// A delegation request from one agent to another.
        Delegation => "delegation",
        /// A broadcast message visible to all agents in the run.
        Broadcast => "broadcast",
    }
}

/// A message in the queue.
#[derive(Debug, Clone)]
pub struct QueueMessage {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub sender: String,
    pub recipient: String,
    pub content: String,
    pub msg_type: MessageType,
    pub status: MessageStatus,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub error: Option<String>,
}

/// Aggregate queue counts by message status.
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead: i64,
}

impl QueueMessage {
    pub(crate) fn from_row(row: QueueMessageRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            id: row.id,
            session_key: row.session_key,
            team_run_id: row.team_run_id,
            sender: row.sender,
            recipient: row.recipient,
            content: row.content,
            msg_type: MessageType::parse(&row.msg_type)?,
            status: MessageStatus::parse(&row.status)?,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
            created_at: row.created_at,
            processed_at: row.processed_at,
            error: row.error,
        })
    }
}

/// SQLite-backed message queue for agent-to-agent communication.
pub struct MessageQueue {
    pub(crate) db: Arc<Database>,
}

impl MessageQueue {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
