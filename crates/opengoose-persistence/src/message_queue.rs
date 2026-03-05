use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewQueueMessage, QueueMessageRow};
use crate::schema::message_queue;

/// Status of a queued message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Dead,
}

impl MessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, PersistenceError> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "dead" => Ok(Self::Dead),
            other => Err(PersistenceError::InvalidEnumValue(format!(
                "unknown MessageStatus: {other}"
            ))),
        }
    }
}

/// Type of a queued message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    /// A task to be executed by an agent.
    Task,
    /// A result returned by an agent.
    Result,
    /// A delegation request from one agent to another.
    Delegation,
    /// A broadcast message visible to all agents in the run.
    Broadcast,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Result => "result",
            Self::Delegation => "delegation",
            Self::Broadcast => "broadcast",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, PersistenceError> {
        match s {
            "task" => Ok(Self::Task),
            "result" => Ok(Self::Result),
            "delegation" => Ok(Self::Delegation),
            "broadcast" => Ok(Self::Broadcast),
            other => Err(PersistenceError::InvalidEnumValue(format!(
                "unknown MessageType: {other}"
            ))),
        }
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

impl QueueMessage {
    fn from_row(row: QueueMessageRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            id: row.id,
            session_key: row.session_key,
            team_run_id: row.team_run_id,
            sender: row.sender,
            recipient: row.recipient,
            content: row.content,
            msg_type: MessageType::from_str(&row.msg_type)?,
            status: MessageStatus::from_str(&row.status)?,
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
    db: Arc<Database>,
}

impl MessageQueue {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Add a message to the queue.
    pub fn enqueue(
        &self,
        session_key: &str,
        team_run_id: &str,
        sender: &str,
        recipient: &str,
        content: &str,
        msg_type: MessageType,
    ) -> PersistenceResult<i32> {
        self.db.with(|conn| {
            // Deduplicate broadcasts: same (team_run_id, sender, content) → return existing ID
            if msg_type == MessageType::Broadcast {
                let existing = message_queue::table
                    .filter(message_queue::team_run_id.eq(team_run_id))
                    .filter(message_queue::sender.eq(sender))
                    .filter(message_queue::content.eq(content))
                    .filter(message_queue::msg_type.eq(MessageType::Broadcast.as_str()))
                    .select(message_queue::id)
                    .first::<i32>(conn)
                    .optional()?;
                if let Some(id) = existing {
                    debug!(id, sender, "duplicate broadcast suppressed");
                    return Ok(id);
                }
            }

            let row = diesel::insert_into(message_queue::table)
                .values(NewQueueMessage {
                    session_key,
                    team_run_id,
                    sender,
                    recipient,
                    content,
                    msg_type: msg_type.as_str(),
                })
                .get_result::<QueueMessageRow>(conn)?;
            debug!(id = row.id, sender, recipient, "message enqueued");
            Ok(row.id)
        })
    }

    /// Atomically fetch pending messages matching a filter and mark them as processing.
    fn dequeue_filtered(
        conn: &mut SqliteConnection,
        rows: Vec<QueueMessageRow>,
    ) -> Result<Vec<QueueMessage>, diesel::result::Error> {
        let messages: Vec<QueueMessage> = rows
            .into_iter()
            .map(QueueMessage::from_row)
            .collect::<Result<_, _>>()
            .map_err(|e| diesel::result::Error::QueryBuilderError(Box::new(e)))?;

        if !messages.is_empty() {
            let ids: Vec<i32> = messages.iter().map(|m| m.id).collect();
            diesel::update(message_queue::table.filter(message_queue::id.eq_any(&ids)))
                .set((
                    message_queue::status.eq(MessageStatus::Processing.as_str()),
                    message_queue::processed_at.eq(db::now_sql_nullable()),
                ))
                .execute(conn)?;
        }

        Ok(messages)
    }

    /// Atomically dequeue pending messages for a recipient (marks them as processing).
    pub fn dequeue(&self, recipient: &str, limit: usize) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let result: Result<Vec<QueueMessage>, diesel::result::Error> =
                conn.transaction(|conn| {
                    let rows = message_queue::table
                        .filter(message_queue::recipient.eq(recipient))
                        .filter(message_queue::status.eq(MessageStatus::Pending.as_str()))
                        .order(message_queue::created_at.asc())
                        .limit(limit as i64)
                        .load::<QueueMessageRow>(conn)?;
                    let messages = Self::dequeue_filtered(conn, rows)?;
                    debug!(count = messages.len(), recipient, "messages dequeued");
                    Ok(messages)
                });
            result.map_err(Into::into)
        })
    }

    /// Mark a message as completed.
    pub fn complete(&self, message_id: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(message_queue::table.find(message_id))
                .set((
                    message_queue::status.eq(MessageStatus::Completed.as_str()),
                    message_queue::processed_at.eq(db::now_sql_nullable()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Mark a message as failed. Retries if under max_retries, otherwise dead-letters.
    pub fn fail(&self, message_id: i32, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let (retry_count, max_retries) = message_queue::table
                .find(message_id)
                .select((message_queue::retry_count, message_queue::max_retries))
                .first::<(i32, i32)>(conn)?;

            if retry_count + 1 >= max_retries {
                diesel::update(message_queue::table.find(message_id))
                    .set((
                        message_queue::status.eq(MessageStatus::Dead.as_str()),
                        message_queue::error.eq(Some(error)),
                        message_queue::retry_count.eq(retry_count + 1),
                    ))
                    .execute(conn)?;
                debug!(message_id, "message dead-lettered");
            } else {
                diesel::update(message_queue::table.find(message_id))
                    .set((
                        message_queue::status.eq(MessageStatus::Pending.as_str()),
                        message_queue::error.eq(Some(error)),
                        message_queue::retry_count.eq(retry_count + 1),
                        message_queue::processed_at.eq(None::<String>),
                    ))
                    .execute(conn)?;
                debug!(message_id, retry = retry_count + 1, "message retried");
            }
            Ok(())
        })
    }

    /// Read broadcast messages for a team run, optionally since a given message ID.
    pub fn read_broadcasts(
        &self,
        team_run_id: &str,
        since_id: Option<i32>,
    ) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let since = since_id.unwrap_or(0);
            let rows = message_queue::table
                .filter(message_queue::team_run_id.eq(team_run_id))
                .filter(message_queue::msg_type.eq(MessageType::Broadcast.as_str()))
                .filter(message_queue::id.gt(since))
                .order(message_queue::created_at.asc())
                .load::<QueueMessageRow>(conn)?;
            rows.into_iter()
                .map(QueueMessage::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Atomically dequeue pending delegation messages for a team run.
    pub fn dequeue_delegations(
        &self,
        team_run_id: &str,
        limit: usize,
    ) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let result: Result<Vec<QueueMessage>, diesel::result::Error> =
                conn.transaction(|conn| {
                    let rows = message_queue::table
                        .filter(message_queue::team_run_id.eq(team_run_id))
                        .filter(message_queue::msg_type.eq(MessageType::Delegation.as_str()))
                        .filter(message_queue::status.eq(MessageStatus::Pending.as_str()))
                        .order(message_queue::created_at.asc())
                        .limit(limit as i64)
                        .load::<QueueMessageRow>(conn)?;
                    let messages = Self::dequeue_filtered(conn, rows)?;
                    debug!(count = messages.len(), team_run_id, "delegations dequeued");
                    Ok(messages)
                });
            result.map_err(Into::into)
        })
    }

    /// Get dead-lettered messages for a team run (for user reporting).
    pub fn get_dead_letters(&self, team_run_id: &str) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let rows = message_queue::table
                .filter(message_queue::team_run_id.eq(team_run_id))
                .filter(message_queue::status.eq(MessageStatus::Dead.as_str()))
                .order(message_queue::created_at.asc())
                .load::<QueueMessageRow>(conn)?;
            rows.into_iter()
                .map(QueueMessage::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Get all messages for a team run (useful for debugging/TUI).
    pub fn list_for_run(&self, team_run_id: &str) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let rows = message_queue::table
                .filter(message_queue::team_run_id.eq(team_run_id))
                .order(message_queue::created_at.asc())
                .load::<QueueMessageRow>(conn)?;
            rows.into_iter()
                .map(QueueMessage::from_row)
                .collect::<Result<_, _>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn test_enqueue_dequeue() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id = mq
            .enqueue("sess1", "run1", "user", "coder", "fix this bug", MessageType::Task)
            .unwrap();
        assert!(id > 0);

        // Dequeue for wrong recipient → empty
        let msgs = mq.dequeue("reviewer", 10).unwrap();
        assert!(msgs.is_empty());

        // Dequeue for correct recipient
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "fix this bug");
        assert_eq!(msgs[0].status, MessageStatus::Pending);

        // Dequeue again → empty (already processing)
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_complete() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id = mq
            .enqueue("sess1", "run1", "user", "coder", "task1", MessageType::Task)
            .unwrap();
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert_eq!(msgs.len(), 1);

        mq.complete(id).unwrap();

        let msgs = mq.dequeue("coder", 10).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_fail_and_retry() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id = mq
            .enqueue("sess1", "run1", "user", "coder", "task1", MessageType::Task)
            .unwrap();
        mq.dequeue("coder", 10).unwrap();

        // Fail → should go back to pending (retry_count 1 < max_retries 3)
        mq.fail(id, "timeout").unwrap();
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].retry_count, 1);

        // Fail again
        mq.fail(id, "timeout 2").unwrap();
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].retry_count, 2);

        // Fail third time → dead
        mq.fail(id, "timeout 3").unwrap();
        let msgs = mq.dequeue("coder", 10).unwrap();
        assert!(msgs.is_empty()); // dead-lettered
    }

    #[test]
    fn test_broadcasts() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        mq.enqueue("sess1", "run1", "coder", "broadcast", "found issue in auth", MessageType::Broadcast)
            .unwrap();
        let id2 = mq
            .enqueue("sess1", "run1", "reviewer", "broadcast", "tests are passing", MessageType::Broadcast)
            .unwrap();
        // Different run
        mq.enqueue("sess1", "run2", "coder", "broadcast", "other run", MessageType::Broadcast)
            .unwrap();

        let broadcasts = mq.read_broadcasts("run1", None).unwrap();
        assert_eq!(broadcasts.len(), 2);
        assert_eq!(broadcasts[0].content, "found issue in auth");

        // Since id → only newer
        let broadcasts = mq.read_broadcasts("run1", Some(id2 - 1)).unwrap();
        assert_eq!(broadcasts.len(), 1);
        assert_eq!(broadcasts[0].content, "tests are passing");
    }

    #[test]
    fn test_broadcast_deduplication() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id1 = mq
            .enqueue("s1", "run1", "coder", "broadcast", "found bug", MessageType::Broadcast)
            .unwrap();
        let id2 = mq
            .enqueue("s1", "run1", "coder", "broadcast", "found bug", MessageType::Broadcast)
            .unwrap();
        assert_eq!(id1, id2);

        let broadcasts = mq.read_broadcasts("run1", None).unwrap();
        assert_eq!(broadcasts.len(), 1);

        // Different sender, same content → not a duplicate
        mq.enqueue("s1", "run1", "reviewer", "broadcast", "found bug", MessageType::Broadcast)
            .unwrap();
        let broadcasts = mq.read_broadcasts("run1", None).unwrap();
        assert_eq!(broadcasts.len(), 2);

        // Same sender, different content → not a duplicate
        mq.enqueue("s1", "run1", "coder", "broadcast", "found another bug", MessageType::Broadcast)
            .unwrap();
        let broadcasts = mq.read_broadcasts("run1", None).unwrap();
        assert_eq!(broadcasts.len(), 3);
    }

    #[test]
    fn test_dequeue_delegations() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        mq.enqueue("s1", "run1", "coder", "reviewer", "check auth", MessageType::Delegation)
            .unwrap();
        mq.enqueue("s1", "run1", "coder", "tester", "run tests", MessageType::Delegation)
            .unwrap();
        mq.enqueue("s1", "run1", "user", "coder", "fix bug", MessageType::Task)
            .unwrap();
        mq.enqueue("s1", "run2", "coder", "reviewer", "other run", MessageType::Delegation)
            .unwrap();

        let msgs = mq.dequeue_delegations("run1", 10).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "check auth");
        assert_eq!(msgs[0].recipient, "reviewer");
        assert_eq!(msgs[1].content, "run tests");
        assert_eq!(msgs[1].recipient, "tester");

        let msgs = mq.dequeue_delegations("run1", 10).unwrap();
        assert!(msgs.is_empty());

        let msgs = mq.dequeue_delegations("run2", 10).unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_dequeue_delegations_only_pending() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id1 = mq
            .enqueue("s1", "run1", "coder", "reviewer", "msg1", MessageType::Delegation)
            .unwrap();
        mq.enqueue("s1", "run1", "coder", "tester", "msg2", MessageType::Delegation)
            .unwrap();

        let msgs = mq.dequeue_delegations("run1", 1).unwrap();
        assert_eq!(msgs.len(), 1);
        mq.complete(id1).unwrap();

        let msgs = mq.dequeue_delegations("run1", 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "msg2");
    }

    #[test]
    fn test_get_dead_letters() {
        let db = test_db();
        let mq = MessageQueue::new(db);

        let id = mq
            .enqueue("s1", "run1", "coder", "reviewer", "bad task", MessageType::Delegation)
            .unwrap();

        mq.dequeue("reviewer", 10).unwrap();
        mq.fail(id, "err1").unwrap();
        mq.dequeue("reviewer", 10).unwrap();
        mq.fail(id, "err2").unwrap();
        mq.dequeue("reviewer", 10).unwrap();
        mq.fail(id, "err3").unwrap();

        let dead = mq.get_dead_letters("run1").unwrap();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].content, "bad task");
        assert_eq!(dead[0].status, MessageStatus::Dead);

        let dead = mq.get_dead_letters("run2").unwrap();
        assert!(dead.is_empty());
    }
}
