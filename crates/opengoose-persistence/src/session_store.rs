use std::collections::HashMap;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::sql_types::Text;
use tracing::{debug, info};

use opengoose_types::SessionKey;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewMessage, NewSession};
use crate::schema::{messages, sessions};

/// A session row returned by list queries.
#[derive(Debug, Clone)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Alias for backward compatibility with code using SessionSummary.
pub type SessionSummary = SessionItem;

/// Aggregate session statistics.
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub session_count: i64,
    pub message_count: i64,
}

/// A conversation message stored in the database.
#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

/// Session and conversation history operations on a shared Database.
pub struct SessionStore {
    db: Arc<Database>,
}

impl SessionStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Upsert a session row: insert if missing, update `updated_at` if exists.
    fn upsert_session(conn: &mut SqliteConnection, key: &str) -> PersistenceResult<()> {
        diesel::insert_into(sessions::table)
            .values(NewSession { session_key: key })
            .on_conflict(sessions::session_key)
            .do_update()
            .set(sessions::updated_at.eq(db::now_sql()))
            .execute(conn)?;
        Ok(())
    }

    /// Append a message to the conversation history (shared implementation).
    fn append_message(
        &self,
        key: &SessionKey,
        role: &str,
        content: &str,
        author: Option<&str>,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            conn.transaction(|conn| {
                Self::upsert_session(conn, &key_str)?;
                diesel::insert_into(messages::table)
                    .values(NewMessage {
                        session_key: &key_str,
                        role,
                        content,
                        author,
                    })
                    .execute(conn)?;
                debug!(%key, role, "appended message");
                Ok(())
            })
        })
    }

    /// Append a user message to the conversation history.
    pub fn append_user_message(
        &self,
        key: &SessionKey,
        content: &str,
        author: Option<&str>,
    ) -> PersistenceResult<()> {
        self.append_message(key, "user", content, author)
    }

    /// Append an assistant message to the conversation history.
    pub fn append_assistant_message(
        &self,
        key: &SessionKey,
        content: &str,
    ) -> PersistenceResult<()> {
        self.append_message(key, "assistant", content, Some("goose"))
    }

    /// Load the most recent messages for a session.
    pub fn load_history(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            let rows = messages::table
                .filter(messages::session_key.eq(&key_str))
                .order((messages::created_at.desc(), messages::id.desc()))
                .limit(limit as i64)
                .select((
                    messages::role,
                    messages::content,
                    messages::author,
                    messages::created_at,
                ))
                .load::<(String, String, Option<String>, String)>(conn)?;
            let mut messages: Vec<HistoryMessage> = rows
                .into_iter()
                .map(|(role, content, author, created_at)| HistoryMessage {
                    role,
                    content,
                    author,
                    created_at,
                })
                .collect();
            messages.reverse();
            Ok(messages)
        })
    }

    /// Set or clear the active team for a session.
    pub fn set_active_team(&self, key: &SessionKey, team: Option<&str>) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            diesel::insert_into(sessions::table)
                .values((
                    sessions::session_key.eq(&key_str),
                    sessions::active_team.eq(team),
                ))
                .on_conflict(sessions::session_key)
                .do_update()
                .set((
                    sessions::active_team.eq(team),
                    sessions::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Get the active team for a session.
    pub fn get_active_team(&self, key: &SessionKey) -> PersistenceResult<Option<String>> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            let result = sessions::table
                .filter(sessions::session_key.eq(&key_str))
                .select(sessions::active_team)
                .first::<Option<String>>(conn)
                .optional()?;
            Ok(result.flatten())
        })
    }

    /// Load all sessions that have an active team set.
    pub fn load_all_active_teams(&self) -> PersistenceResult<HashMap<SessionKey, String>> {
        self.db.with(|conn| {
            let rows = sessions::table
                .filter(sessions::active_team.is_not_null())
                .select((sessions::session_key, sessions::active_team))
                .load::<(String, Option<String>)>(conn)?;
            let mut map = HashMap::new();
            for (key_str, team) in rows {
                if let Some(team) = team {
                    map.insert(SessionKey::from_stable_id(&key_str), team);
                }
            }
            Ok(map)
        })
    }

    /// List sessions ordered by most recently updated, limited to `limit` results.
    pub fn list_sessions(&self, limit: i64) -> PersistenceResult<Vec<SessionItem>> {
        self.db.with(|conn| {
            let rows = sessions::table
                .order(sessions::updated_at.desc())
                .limit(limit)
                .select((
                    sessions::session_key,
                    sessions::active_team,
                    sessions::created_at,
                    sessions::updated_at,
                ))
                .load::<(String, Option<String>, String, String)>(conn)?;
            Ok(rows
                .into_iter()
                .map(
                    |(session_key, active_team, created_at, updated_at)| SessionItem {
                        session_key,
                        active_team,
                        created_at,
                        updated_at,
                    },
                )
                .collect())
        })
    }

    /// Return aggregate statistics (session count and message count).
    pub fn stats(&self) -> PersistenceResult<SessionStats> {
        self.db.with(|conn| {
            let session_count = sessions::table.count().get_result::<i64>(conn)?;
            let message_count = messages::table.count().get_result::<i64>(conn)?;
            Ok(SessionStats {
                session_count,
                message_count,
            })
        })
    }

    /// Delete sessions and messages older than the given number of hours.
    pub fn cleanup(&self, max_age_hours: i64) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let cutoff = format!("-{max_age_hours} hours");
            diesel::sql_query(
                "DELETE FROM messages WHERE session_key IN (
                    SELECT session_key FROM sessions
                    WHERE updated_at < datetime('now', ?1)
                )",
            )
            .bind::<Text, _>(&cutoff)
            .execute(conn)?;
            let deleted =
                diesel::sql_query("DELETE FROM sessions WHERE updated_at < datetime('now', ?1)")
                    .bind::<Text, _>(&cutoff)
                    .execute(conn)?;
            if deleted > 0 {
                info!(deleted, "cleaned up old sessions");
            }
            Ok(deleted)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::Platform;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn test_key() -> SessionKey {
        SessionKey::new(Platform::Discord, "guild123".to_string(), "channel456")
    }

    #[test]
    fn test_append_and_load_history() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        store
            .append_user_message(&key, "hello", Some("alice"))
            .unwrap();
        store.append_assistant_message(&key, "hi there").unwrap();
        store
            .append_user_message(&key, "how are you?", Some("alice"))
            .unwrap();

        let history = store.load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "hi there");
        assert_eq!(history[2].role, "user");
        assert_eq!(history[2].content, "how are you?");
    }

    #[test]
    fn test_load_history_limit() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        for i in 0..10 {
            store
                .append_user_message(&key, &format!("msg {i}"), None)
                .unwrap();
        }

        let history = store.load_history(&key, 3).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].content, "msg 7");
        assert_eq!(history[2].content, "msg 9");
    }

    #[test]
    fn test_active_team() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        assert_eq!(store.get_active_team(&key).unwrap(), None);

        store.set_active_team(&key, Some("code-review")).unwrap();
        assert_eq!(
            store.get_active_team(&key).unwrap(),
            Some("code-review".into())
        );

        store.set_active_team(&key, None).unwrap();
        assert_eq!(store.get_active_team(&key).unwrap(), None);
    }

    #[test]
    fn test_load_all_active_teams() {
        let store = SessionStore::new(test_db());

        let key1 = SessionKey::new(Platform::Discord, "g1".to_string(), "c1");
        let key2 = SessionKey::new(Platform::Discord, "g2".to_string(), "c2");
        let key3 = SessionKey::new(Platform::Discord, "g3".to_string(), "c3");

        store.set_active_team(&key1, Some("team-a")).unwrap();
        store.set_active_team(&key2, Some("team-b")).unwrap();
        store.set_active_team(&key3, None).unwrap();

        let teams = store.load_all_active_teams().unwrap();
        assert_eq!(teams.len(), 2);
        assert_eq!(teams.get(&key1).unwrap(), "team-a");
        assert_eq!(teams.get(&key2).unwrap(), "team-b");
    }

    #[test]
    fn test_cleanup() {
        let db = test_db();
        let store = SessionStore::new(db.clone());
        let key = test_key();

        store.append_user_message(&key, "old msg", None).unwrap();

        db.with(|conn| {
            diesel::sql_query("UPDATE sessions SET updated_at = datetime('now', '-100 hours')")
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let deleted = store.cleanup(72).unwrap();
        assert_eq!(deleted, 1);

        let history = store.load_history(&key, 10).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_load_history_nonexistent_session() {
        let store = SessionStore::new(test_db());
        let key = SessionKey::new(Platform::Telegram, "unknown_guild", "unknown_chan");
        let history = store.load_history(&key, 10).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_load_history_preserves_author() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        store
            .append_user_message(&key, "hello", Some("alice"))
            .unwrap();
        store.append_user_message(&key, "no author", None).unwrap();
        store.append_assistant_message(&key, "reply").unwrap();

        let history = store.load_history(&key, 10).unwrap();
        assert_eq!(history[0].author.as_deref(), Some("alice"));
        assert_eq!(history[1].author, None);
        assert_eq!(history[2].author.as_deref(), Some("goose"));
    }

    #[test]
    fn test_set_active_team_upserts_session() {
        let store = SessionStore::new(test_db());
        let key = SessionKey::new(Platform::Slack, "ws1", "ch1");
        store.set_active_team(&key, Some("my-team")).unwrap();
        assert_eq!(store.get_active_team(&key).unwrap(), Some("my-team".into()));
    }

    #[test]
    fn test_list_sessions() {
        let store = SessionStore::new(test_db());
        let key1 = SessionKey::new(Platform::Discord, "g1", "c1");
        let key2 = SessionKey::new(Platform::Slack, "ws1", "c2");

        store.append_user_message(&key1, "msg1", None).unwrap();
        store.append_user_message(&key2, "msg2", None).unwrap();

        let sessions = store.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 2);
        // Both sessions should be present (order may vary within the same second)
        let keys: Vec<&str> = sessions.iter().map(|s| s.session_key.as_str()).collect();
        assert!(keys.contains(&key1.to_stable_id().as_str()));
        assert!(keys.contains(&key2.to_stable_id().as_str()));
    }

    #[test]
    fn test_list_sessions_limit() {
        let store = SessionStore::new(test_db());
        for i in 0..5 {
            let key = SessionKey::new(Platform::Discord, format!("g{i}"), format!("c{i}"));
            store.append_user_message(&key, "msg", None).unwrap();
        }
        let sessions = store.list_sessions(2).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_stats() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.message_count, 0);

        store.append_user_message(&key, "hello", None).unwrap();
        store.append_assistant_message(&key, "world").unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.message_count, 2);
    }
}
