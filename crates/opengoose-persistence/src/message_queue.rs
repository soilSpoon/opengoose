use std::sync::Arc;

use rusqlite::params;
use rusqlite::OptionalExtension;
use tracing::debug;

use crate::db::Database;
use crate::error::{PersistenceError, PersistenceResult};

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
    pub id: i64,
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

/// Parse a QueueMessage from a SQLite row.
fn row_to_queue_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueMessage> {
    Ok(QueueMessage {
        id: row.get(0)?,
        session_key: row.get(1)?,
        team_run_id: row.get(2)?,
        sender: row.get(3)?,
        recipient: row.get(4)?,
        content: row.get(5)?,
        msg_type: MessageType::from_str(&row.get::<_, String>(6)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        status: MessageStatus::from_str(&row.get::<_, String>(7)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        retry_count: row.get(8)?,
        max_retries: row.get(9)?,
        created_at: row.get(10)?,
        processed_at: row.get(11)?,
        error: row.get(12)?,
    })
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
    ) -> PersistenceResult<i64> {
        self.db.with(|conn| {
            // Deduplicate broadcasts: same (team_run_id, sender, content) → return existing ID
            if msg_type == MessageType::Broadcast {
                let existing: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM message_queue
                         WHERE team_run_id = ?1 AND sender = ?2 AND content = ?3 AND msg_type = 'broadcast'
                         LIMIT 1",
                        params![team_run_id, sender, content],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(id) = existing {
                    debug!(id, sender, "duplicate broadcast suppressed");
                    return Ok(id);
                }
            }

            conn.execute(
                "INSERT INTO message_queue (session_key, team_run_id, sender, recipient, content, msg_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_key, team_run_id, sender, recipient, content, msg_type.as_str()],
            )?;
            let id = conn.last_insert_rowid();
            debug!(id, sender, recipient, "message enqueued");
            Ok(id)
        })
    }

    /// Atomically dequeue pending messages for a recipient (marks them as processing).
    pub fn dequeue(&self, recipient: &str, limit: usize) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let tx = conn.unchecked_transaction()?;

            // Scope the statement so it's dropped before tx.commit()
            let messages: Vec<QueueMessage> = {
                let mut stmt = tx.prepare(
                    "SELECT id, session_key, team_run_id, sender, recipient, content, msg_type,
                            status, retry_count, max_retries, created_at, processed_at, error
                     FROM message_queue
                     WHERE recipient = ?1 AND status = 'pending'
                     ORDER BY created_at ASC
                     LIMIT ?2",
                )?;
                stmt.query_map(params![recipient, limit as i64], row_to_queue_message)?
                .collect::<Result<_, _>>()?
            };

            // Mark them as processing
            for msg in &messages {
                tx.execute(
                    "UPDATE message_queue SET status = 'processing', processed_at = datetime('now') WHERE id = ?1",
                    params![msg.id],
                )?;
            }
            tx.commit()?;

            debug!(count = messages.len(), recipient, "messages dequeued");
            Ok(messages)
        })
    }

    /// Mark a message as completed.
    pub fn complete(&self, message_id: i64) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE message_queue SET status = 'completed', processed_at = datetime('now') WHERE id = ?1",
                params![message_id],
            )?;
            Ok(())
        })
    }

    /// Mark a message as failed. Retries if under max_retries, otherwise dead-letters.
    pub fn fail(&self, message_id: i64, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let (retry_count, max_retries): (i32, i32) = conn.query_row(
                "SELECT retry_count, max_retries FROM message_queue WHERE id = ?1",
                params![message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;

            if retry_count + 1 >= max_retries {
                conn.execute(
                    "UPDATE message_queue SET status = 'dead', error = ?1, retry_count = retry_count + 1 WHERE id = ?2",
                    params![error, message_id],
                )?;
                debug!(message_id, "message dead-lettered");
            } else {
                conn.execute(
                    "UPDATE message_queue SET status = 'pending', error = ?1, retry_count = retry_count + 1, processed_at = NULL WHERE id = ?2",
                    params![error, message_id],
                )?;
                debug!(message_id, retry = retry_count + 1, "message retried");
            }
            Ok(())
        })
    }

    /// Read broadcast messages for a team run, optionally since a given message ID.
    pub fn read_broadcasts(
        &self,
        team_run_id: &str,
        since_id: Option<i64>,
    ) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let since = since_id.unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT id, session_key, team_run_id, sender, recipient, content, msg_type,
                        status, retry_count, max_retries, created_at, processed_at, error
                 FROM message_queue
                 WHERE team_run_id = ?1 AND msg_type = 'broadcast' AND id > ?2
                 ORDER BY created_at ASC",
            )?;
            let messages: Vec<QueueMessage> = stmt
                .query_map(params![team_run_id, since], row_to_queue_message)?
                .collect::<Result<_, _>>()?;
            Ok(messages)
        })
    }

    /// Atomically dequeue pending delegation messages for a team run.
    ///
    /// Unlike `dequeue()` which filters by recipient, this selects all pending
    /// delegations for the entire run — used by the post-workflow consumer loop.
    pub fn dequeue_delegations(
        &self,
        team_run_id: &str,
        limit: usize,
    ) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let tx = conn.unchecked_transaction()?;

            let messages: Vec<QueueMessage> = {
                let mut stmt = tx.prepare(
                    "SELECT id, session_key, team_run_id, sender, recipient, content, msg_type,
                            status, retry_count, max_retries, created_at, processed_at, error
                     FROM message_queue
                     WHERE team_run_id = ?1 AND msg_type = 'delegation' AND status = 'pending'
                     ORDER BY created_at ASC
                     LIMIT ?2",
                )?;
                stmt.query_map(params![team_run_id, limit as i64], row_to_queue_message)?
                .collect::<Result<_, _>>()?
            };

            for msg in &messages {
                tx.execute(
                    "UPDATE message_queue SET status = 'processing', processed_at = datetime('now') WHERE id = ?1",
                    params![msg.id],
                )?;
            }
            tx.commit()?;

            debug!(count = messages.len(), team_run_id, "delegations dequeued");
            Ok(messages)
        })
    }

    /// Get dead-lettered messages for a team run (for user reporting).
    pub fn get_dead_letters(&self, team_run_id: &str) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_key, team_run_id, sender, recipient, content, msg_type,
                        status, retry_count, max_retries, created_at, processed_at, error
                 FROM message_queue
                 WHERE team_run_id = ?1 AND status = 'dead'
                 ORDER BY created_at ASC",
            )?;
            let messages: Vec<QueueMessage> = stmt
                .query_map(params![team_run_id], row_to_queue_message)?
                .collect::<Result<_, _>>()?;
            Ok(messages)
        })
    }

    /// Get all messages for a team run (useful for debugging/TUI).
    pub fn list_for_run(&self, team_run_id: &str) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_key, team_run_id, sender, recipient, content, msg_type,
                        status, retry_count, max_retries, created_at, processed_at, error
                 FROM message_queue
                 WHERE team_run_id = ?1
                 ORDER BY created_at ASC",
            )?;
            let messages: Vec<QueueMessage> = stmt
                .query_map(params![team_run_id], row_to_queue_message)?
                .collect::<Result<_, _>>()?;
            Ok(messages)
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
        // Status in the returned struct reflects the SELECT before UPDATE
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

        // Can't dequeue completed
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
        // Same ID returned (deduplicated)
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

        // Mix of delegation and task messages
        mq.enqueue("s1", "run1", "coder", "reviewer", "check auth", MessageType::Delegation)
            .unwrap();
        mq.enqueue("s1", "run1", "coder", "tester", "run tests", MessageType::Delegation)
            .unwrap();
        mq.enqueue("s1", "run1", "user", "coder", "fix bug", MessageType::Task)
            .unwrap();
        // Different run
        mq.enqueue("s1", "run2", "coder", "reviewer", "other run", MessageType::Delegation)
            .unwrap();

        // Only delegations for run1
        let msgs = mq.dequeue_delegations("run1", 10).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "check auth");
        assert_eq!(msgs[0].recipient, "reviewer");
        assert_eq!(msgs[1].content, "run tests");
        assert_eq!(msgs[1].recipient, "tester");

        // Already processing → empty on second call
        let msgs = mq.dequeue_delegations("run1", 10).unwrap();
        assert!(msgs.is_empty());

        // run2 unaffected
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

        // Complete first delegation
        let msgs = mq.dequeue_delegations("run1", 1).unwrap();
        assert_eq!(msgs.len(), 1);
        mq.complete(id1).unwrap();

        // Only second delegation remains
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

        // Fail 3 times to dead-letter
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

        // Different run → empty
        let dead = mq.get_dead_letters("run2").unwrap();
        assert!(dead.is_empty());
    }
}
