use diesel::prelude::*;
use tracing::debug;

use crate::db;
use crate::error::PersistenceResult;
use crate::models::{NewQueueMessage, QueueMessageRow};
use crate::schema::message_queue;

use super::{MessageQueue, MessageStatus, MessageType, QueueMessage, QueueStats};

impl MessageQueue {
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

    /// List recent queue activity across all team runs.
    pub fn list_recent(&self, limit: usize) -> PersistenceResult<Vec<QueueMessage>> {
        self.db.with(|conn| {
            let rows = message_queue::table
                .order((message_queue::created_at.desc(), message_queue::id.desc()))
                .limit(limit as i64)
                .load::<QueueMessageRow>(conn)?;
            rows.into_iter()
                .map(QueueMessage::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Count queued messages by processing status.
    pub fn stats(&self) -> PersistenceResult<QueueStats> {
        self.db.with(|conn| {
            let count_status = |status: MessageStatus,
                                conn: &mut SqliteConnection|
             -> Result<i64, diesel::result::Error> {
                message_queue::table
                    .filter(message_queue::status.eq(status.as_str()))
                    .count()
                    .get_result(conn)
            };

            Ok(QueueStats {
                pending: count_status(MessageStatus::Pending, conn)?,
                processing: count_status(MessageStatus::Processing, conn)?,
                completed: count_status(MessageStatus::Completed, conn)?,
                failed: count_status(MessageStatus::Failed, conn)?,
                dead: count_status(MessageStatus::Dead, conn)?,
            })
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
