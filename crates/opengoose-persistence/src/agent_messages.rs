use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::Database;
use crate::db_enum::db_enum;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{AgentMessageRow, NewAgentMessage};
use crate::schema::agent_messages;

db_enum! {
    /// Delivery status of an agent message.
    pub enum AgentMessageStatus {
        Pending => "pending",
        Delivered => "delivered",
        Acknowledged => "acknowledged",
    }
}

/// A persisted agent message (directed or pub/sub channel).
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub id: i32,
    pub session_key: String,
    pub from_agent: String,
    /// `None` for channel messages (broadcast to subscribers).
    pub to_agent: Option<String>,
    /// `None` for directed messages (point-to-point).
    pub channel: Option<String>,
    pub payload: String,
    pub status: AgentMessageStatus,
    pub created_at: String,
    pub delivered_at: Option<String>,
}

impl AgentMessage {
    fn from_row(row: AgentMessageRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            id: row.id,
            session_key: row.session_key,
            from_agent: row.from_agent,
            to_agent: row.to_agent,
            channel: row.channel,
            payload: row.payload,
            status: AgentMessageStatus::parse(&row.status)?,
            created_at: row.created_at,
            delivered_at: row.delivered_at,
        })
    }

    /// Returns true if this is a directed (point-to-point) message.
    pub fn is_directed(&self) -> bool {
        self.to_agent.is_some()
    }

    /// Returns true if this is a channel (pub/sub) message.
    pub fn is_channel(&self) -> bool {
        self.channel.is_some()
    }
}

/// Persistent store for inter-agent messages.
///
/// Supports two communication patterns:
/// - **Directed**: point-to-point messages with `to_agent` set.
/// - **Channel**: pub/sub messages with `channel` set (fan-out).
pub struct AgentMessageStore {
    db: Arc<Database>,
}

impl AgentMessageStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Send a directed message from one agent to another.
    pub fn send_directed(
        &self,
        session_key: &str,
        from: &str,
        to: &str,
        payload: &str,
    ) -> PersistenceResult<i32> {
        debug!(from, to, "sending directed agent message");
        self.insert(session_key, from, Some(to), None, payload)
    }

    /// Publish a message to a named channel (pub/sub broadcast).
    pub fn publish(
        &self,
        session_key: &str,
        from: &str,
        channel: &str,
        payload: &str,
    ) -> PersistenceResult<i32> {
        debug!(from, channel, "publishing to agent channel");
        self.insert(session_key, from, None, Some(channel), payload)
    }

    fn insert(
        &self,
        session_key: &str,
        from_agent: &str,
        to_agent: Option<&str>,
        channel: Option<&str>,
        payload: &str,
    ) -> PersistenceResult<i32> {
        let new_msg = NewAgentMessage {
            session_key,
            from_agent,
            to_agent,
            channel,
            payload,
        };
        self.db.with(|conn| {
            let row = diesel::insert_into(agent_messages::table)
                .values(&new_msg)
                .get_result::<AgentMessageRow>(conn)?;
            debug!(id = row.id, from_agent, "agent message stored");
            Ok(row.id)
        })
    }

    /// Retrieve all pending directed messages for a given recipient.
    pub fn receive_pending(
        &self,
        session_key: &str,
        to_agent: &str,
    ) -> PersistenceResult<Vec<AgentMessage>> {
        self.db.with(|conn| {
            let rows = agent_messages::table
                .filter(agent_messages::session_key.eq(session_key))
                .filter(agent_messages::to_agent.eq(to_agent))
                .filter(agent_messages::status.eq("pending"))
                .order(agent_messages::id.asc())
                .load::<AgentMessageRow>(conn)?;
            rows.into_iter()
                .map(AgentMessage::from_row)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// Retrieve channel messages (optionally since a given id).
    pub fn channel_history(
        &self,
        session_key: &str,
        channel: &str,
        since_id: Option<i32>,
    ) -> PersistenceResult<Vec<AgentMessage>> {
        self.db.with(|conn| {
            let mut query = agent_messages::table
                .filter(agent_messages::session_key.eq(session_key))
                .filter(agent_messages::channel.eq(channel))
                .order(agent_messages::id.asc())
                .into_boxed();
            if let Some(id) = since_id {
                query = query.filter(agent_messages::id.gt(id));
            }
            let rows = query.load::<AgentMessageRow>(conn)?;
            rows.into_iter()
                .map(AgentMessage::from_row)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// Mark a message as delivered.
    pub fn mark_delivered(&self, id: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(agent_messages::table.find(id))
                .set((
                    agent_messages::status.eq("delivered"),
                    agent_messages::delivered_at.eq(diesel::dsl::sql::<
                        diesel::sql_types::Nullable<diesel::sql_types::Text>,
                    >("datetime('now')")),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Mark a message as acknowledged by the recipient.
    pub fn acknowledge(&self, id: i32) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(agent_messages::table.find(id))
                .set(agent_messages::status.eq("acknowledged"))
                .execute(conn)?;
            Ok(())
        })
    }

    /// List recent messages for a session (most recent first).
    pub fn list_recent(
        &self,
        session_key: &str,
        limit: i64,
    ) -> PersistenceResult<Vec<AgentMessage>> {
        self.db.with(|conn| {
            let rows = agent_messages::table
                .filter(agent_messages::session_key.eq(session_key))
                .order(agent_messages::id.desc())
                .limit(limit)
                .load::<AgentMessageRow>(conn)?;
            rows.into_iter()
                .map(AgentMessage::from_row)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// List recent messages across all sessions (most recent first).
    pub fn list_recent_global(&self, limit: i64) -> PersistenceResult<Vec<AgentMessage>> {
        self.db.with(|conn| {
            let rows = agent_messages::table
                .order(agent_messages::id.desc())
                .limit(limit)
                .load::<AgentMessageRow>(conn)?;
            rows.into_iter()
                .map(AgentMessage::from_row)
                .collect::<Result<Vec<_>, _>>()
        })
    }

    /// List all messages exchanged with a specific agent (as sender or recipient).
    pub fn list_for_agent(
        &self,
        session_key: &str,
        agent_name: &str,
        limit: i64,
    ) -> PersistenceResult<Vec<AgentMessage>> {
        self.db.with(|conn| {
            let rows = agent_messages::table
                .filter(agent_messages::session_key.eq(session_key))
                .filter(
                    agent_messages::from_agent
                        .eq(agent_name)
                        .or(agent_messages::to_agent.eq(agent_name)),
                )
                .order(agent_messages::id.desc())
                .limit(limit)
                .load::<AgentMessageRow>(conn)?;
            rows.into_iter()
                .map(AgentMessage::from_row)
                .collect::<Result<Vec<_>, _>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> AgentMessageStore {
        let db = Arc::new(Database::open_in_memory().unwrap());
        AgentMessageStore::new(db)
    }

    const SK: &str = "discord:guild:chan";

    fn payloads(messages: &[AgentMessage]) -> Vec<&str> {
        messages.iter().map(|msg| msg.payload.as_str()).collect()
    }

    fn ids(messages: &[AgentMessage]) -> Vec<i32> {
        messages.iter().map(|msg| msg.id).collect()
    }

    #[test]
    fn test_send_directed() {
        let store = test_store();
        let id = store
            .send_directed(SK, "agent-a", "agent-b", "hello from a")
            .unwrap();
        assert!(id > 0);

        let msgs = store.receive_pending(SK, "agent-b").unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].payload, "hello from a");
        assert!(msgs[0].is_directed());
        assert!(!msgs[0].is_channel());
    }

    #[test]
    fn test_publish_channel() {
        let store = test_store();
        let id = store
            .publish(SK, "agent-a", "announcements", "big news!")
            .unwrap();
        assert!(id > 0);

        let history = store.channel_history(SK, "announcements", None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].payload, "big news!");
        assert!(history[0].is_channel());
        assert!(!history[0].is_directed());
    }

    #[test]
    fn test_channel_history_since_id() {
        let store = test_store();
        store.publish(SK, "a", "ch", "first").unwrap();
        let id2 = store.publish(SK, "b", "ch", "second").unwrap();
        store.publish(SK, "c", "ch", "third").unwrap();

        let history = store.channel_history(SK, "ch", Some(id2)).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].payload, "third");
    }

    #[test]
    fn test_receive_pending_filters_by_session_recipient_and_status() {
        let store = test_store();
        let first = store
            .send_directed(SK, "agent-a", "agent-b", "first")
            .unwrap();
        store
            .send_directed(SK, "agent-a", "agent-c", "wrong-recipient")
            .unwrap();
        store
            .publish(SK, "agent-a", "updates", "channel-message")
            .unwrap();
        store
            .send_directed("discord:other:chan", "agent-a", "agent-b", "other-session")
            .unwrap();
        let second = store
            .send_directed(SK, "agent-d", "agent-b", "second")
            .unwrap();
        let delivered = store
            .send_directed(SK, "agent-e", "agent-b", "delivered")
            .unwrap();
        store.mark_delivered(delivered).unwrap();
        let acknowledged = store
            .send_directed(SK, "agent-f", "agent-b", "acknowledged")
            .unwrap();
        store.mark_delivered(acknowledged).unwrap();
        store.acknowledge(acknowledged).unwrap();

        let pending = store.receive_pending(SK, "agent-b").unwrap();
        assert_eq!(payloads(&pending), vec!["first", "second"]);
        assert_eq!(ids(&pending), vec![first, second]);
        assert!(
            pending
                .iter()
                .all(|message| message.status == AgentMessageStatus::Pending)
        );
    }

    #[test]
    fn test_channel_history_filters_by_session_and_channel() {
        let store = test_store();
        let first = store.publish(SK, "agent-a", "ops", "first").unwrap();
        store
            .publish(SK, "agent-a", "alerts", "wrong-channel")
            .unwrap();
        store
            .send_directed(SK, "agent-a", "agent-b", "directed")
            .unwrap();
        store
            .publish("discord:other:chan", "agent-a", "ops", "other-session")
            .unwrap();
        let second = store.publish(SK, "agent-b", "ops", "second").unwrap();

        let history = store.channel_history(SK, "ops", None).unwrap();
        assert_eq!(payloads(&history), vec!["first", "second"]);
        assert_eq!(ids(&history), vec![first, second]);

        let since_first = store.channel_history(SK, "ops", Some(first)).unwrap();
        assert_eq!(payloads(&since_first), vec!["second"]);
        assert_eq!(ids(&since_first), vec![second]);
    }

    #[test]
    fn test_mark_delivered() {
        let store = test_store();
        let id = store.send_directed(SK, "a", "b", "payload").unwrap();

        store.mark_delivered(id).unwrap();

        let pending = store.receive_pending(SK, "b").unwrap();
        assert!(pending.is_empty());

        let recent = store.list_recent(SK, 1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id);
        assert_eq!(recent[0].status, AgentMessageStatus::Delivered);
        assert!(recent[0].delivered_at.is_some());
    }

    #[test]
    fn test_acknowledge() {
        let store = test_store();
        let id = store.send_directed(SK, "a", "b", "msg").unwrap();
        store.mark_delivered(id).unwrap();
        let delivered_at = store.list_recent(SK, 1).unwrap()[0].delivered_at.clone();
        store.acknowledge(id).unwrap();

        let pending = store.receive_pending(SK, "b").unwrap();
        assert!(pending.is_empty());

        let recent = store.list_recent(SK, 1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id);
        assert_eq!(recent[0].status, AgentMessageStatus::Acknowledged);
        assert_eq!(recent[0].delivered_at, delivered_at);
    }

    #[test]
    fn test_list_recent() {
        let store = test_store();
        let oldest = store.send_directed(SK, "a", "b", "msg1").unwrap();
        store.mark_delivered(oldest).unwrap();
        store.acknowledge(oldest).unwrap();
        let middle = store.publish(SK, "a", "ch", "broadcast").unwrap();
        store.mark_delivered(middle).unwrap();
        store.send_directed(SK, "b", "a", "msg2").unwrap();
        store
            .send_directed("discord:other:chan", "x", "y", "other-session")
            .unwrap();

        let recent = store.list_recent(SK, 2).unwrap();
        assert_eq!(payloads(&recent), vec!["msg2", "broadcast"]);
        assert_eq!(recent[0].status, AgentMessageStatus::Pending);
        assert_eq!(recent[1].status, AgentMessageStatus::Delivered);

        let all_recent = store.list_recent(SK, 10).unwrap();
        assert_eq!(payloads(&all_recent), vec!["msg2", "broadcast", "msg1"]);
        assert_eq!(all_recent[2].status, AgentMessageStatus::Acknowledged);
    }

    #[test]
    fn test_list_recent_global() {
        let store = test_store();
        store.send_directed(SK, "a", "b", "msg1").unwrap();
        store
            .publish("discord:other:chan", "c", "updates", "msg2")
            .unwrap();
        store
            .send_directed("slack:team:room", "e", "f", "msg3")
            .unwrap();
        store.publish(SK, "g", "alerts", "msg4").unwrap();

        let recent = store.list_recent_global(3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(payloads(&recent), vec!["msg4", "msg3", "msg2"]);
        assert_eq!(
            recent
                .iter()
                .map(|message| message.session_key.as_str())
                .collect::<Vec<_>>(),
            vec![SK, "slack:team:room", "discord:other:chan"]
        );
    }

    #[test]
    fn test_list_for_agent() {
        let store = test_store();
        store
            .send_directed(SK, "agent-a", "agent-b", "a→b")
            .unwrap();
        store
            .send_directed(SK, "agent-b", "agent-a", "b→a")
            .unwrap();
        store.publish(SK, "agent-a", "ops", "announcement").unwrap();
        store
            .publish(SK, "agent-c", "ops", "unrelated channel")
            .unwrap();
        store
            .send_directed(SK, "agent-c", "agent-d", "c→d")
            .unwrap();
        store
            .send_directed("discord:other:chan", "agent-a", "agent-z", "other-session")
            .unwrap();

        let msgs = store.list_for_agent(SK, "agent-a", 10).unwrap();
        assert_eq!(payloads(&msgs), vec!["announcement", "b→a", "a→b"]);

        let limited = store.list_for_agent(SK, "agent-a", 2).unwrap();
        assert_eq!(payloads(&limited), vec!["announcement", "b→a"]);
    }

    #[test]
    fn test_receive_pending_only_returns_pending() {
        let store = test_store();
        let id = store.send_directed(SK, "a", "b", "msg").unwrap();
        store.mark_delivered(id).unwrap();

        // Already delivered — should not appear in pending
        let pending = store.receive_pending(SK, "b").unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_status_roundtrip() {
        assert_eq!(AgentMessageStatus::Pending.as_str(), "pending");
        assert_eq!(AgentMessageStatus::Delivered.as_str(), "delivered");
        assert_eq!(AgentMessageStatus::Acknowledged.as_str(), "acknowledged");
        assert!(AgentMessageStatus::parse("invalid").is_err());
    }
}
