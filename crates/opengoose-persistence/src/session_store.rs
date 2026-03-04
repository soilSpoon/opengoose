use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::params;
use tracing::{debug, info};

use opengoose_types::SessionKey;

use crate::db::Database;
use crate::error::PersistenceResult;

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

    fn ensure_session(conn: &rusqlite::Connection, key: &str) -> PersistenceResult<()> {
        conn.execute(
            "INSERT OR IGNORE INTO sessions (session_key) VALUES (?1)",
            params![key],
        )?;
        Ok(())
    }

    /// Append a user message to the conversation history.
    pub fn append_user_message(
        &self,
        key: &SessionKey,
        content: &str,
        author: Option<&str>,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            Self::ensure_session(conn, &key_str)?;
            conn.execute(
                "INSERT INTO messages (session_key, role, content, author) VALUES (?1, 'user', ?2, ?3)",
                params![key_str, content, author],
            )?;
            conn.execute(
                "UPDATE sessions SET updated_at = datetime('now') WHERE session_key = ?1",
                params![key_str],
            )?;
            debug!(%key, "appended user message");
            Ok(())
        })
    }

    /// Append an assistant message to the conversation history.
    pub fn append_assistant_message(
        &self,
        key: &SessionKey,
        content: &str,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            Self::ensure_session(conn, &key_str)?;
            conn.execute(
                "INSERT INTO messages (session_key, role, content, author) VALUES (?1, 'assistant', ?2, 'goose')",
                params![key_str, content],
            )?;
            conn.execute(
                "UPDATE sessions SET updated_at = datetime('now') WHERE session_key = ?1",
                params![key_str],
            )?;
            debug!(%key, "appended assistant message");
            Ok(())
        })
    }

    /// Load the most recent messages for a session.
    pub fn load_history(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            let mut stmt = conn.prepare(
                "SELECT role, content, author, created_at
                 FROM messages
                 WHERE session_key = ?1
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![key_str, limit as i64], |row| {
                Ok(HistoryMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    author: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            let mut messages: Vec<HistoryMessage> = rows.collect::<Result<_, _>>()?;
            messages.reverse(); // oldest first
            Ok(messages)
        })
    }

    /// Set or clear the active team for a session.
    pub fn set_active_team(
        &self,
        key: &SessionKey,
        team: Option<&str>,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            Self::ensure_session(conn, &key_str)?;
            conn.execute(
                "UPDATE sessions SET active_team = ?1, updated_at = datetime('now') WHERE session_key = ?2",
                params![team, key_str],
            )?;
            Ok(())
        })
    }

    /// Get the active team for a session.
    pub fn get_active_team(&self, key: &SessionKey) -> PersistenceResult<Option<String>> {
        self.db.with(|conn| {
            let key_str = key.to_stable_id();
            let result = conn.query_row(
                "SELECT active_team FROM sessions WHERE session_key = ?1",
                params![key_str],
                |row| row.get::<_, Option<String>>(0),
            );
            match result {
                Ok(team) => Ok(team),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Load all sessions that have an active team set.
    pub fn load_all_active_teams(&self) -> PersistenceResult<HashMap<SessionKey, String>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT session_key, active_team FROM sessions WHERE active_team IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |row| {
                let key_str: String = row.get(0)?;
                let team: String = row.get(1)?;
                Ok((key_str, team))
            })?;
            let mut map = HashMap::new();
            for row in rows {
                let (key_str, team) = row?;
                map.insert(SessionKey::from_stable_id(&key_str), team);
            }
            Ok(map)
        })
    }

    /// Delete sessions and messages older than the given number of hours.
    pub fn cleanup(&self, max_age_hours: i64) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let cutoff = format!("-{max_age_hours} hours");
            conn.execute(
                "DELETE FROM messages WHERE session_key IN (
                    SELECT session_key FROM sessions
                    WHERE updated_at < datetime('now', ?1)
                )",
                params![cutoff],
            )?;
            let deleted = conn.execute(
                "DELETE FROM sessions WHERE updated_at < datetime('now', ?1)",
                params![cutoff],
            )?;
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

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn test_key() -> SessionKey {
        SessionKey::new("guild123".to_string(), "channel456")
    }

    #[test]
    fn test_append_and_load_history() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        store.append_user_message(&key, "hello", Some("alice")).unwrap();
        store.append_assistant_message(&key, "hi there").unwrap();
        store.append_user_message(&key, "how are you?", Some("alice")).unwrap();

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
            store.append_user_message(&key, &format!("msg {i}"), None).unwrap();
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
        assert_eq!(store.get_active_team(&key).unwrap(), Some("code-review".into()));

        store.set_active_team(&key, None).unwrap();
        assert_eq!(store.get_active_team(&key).unwrap(), None);
    }

    #[test]
    fn test_load_all_active_teams() {
        let store = SessionStore::new(test_db());

        let key1 = SessionKey::new("g1".to_string(), "c1");
        let key2 = SessionKey::new("g2".to_string(), "c2");
        let key3 = SessionKey::new("g3".to_string(), "c3");

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

        // Backdate the session so cleanup can find it
        db.with(|conn| {
            conn.execute(
                "UPDATE sessions SET updated_at = datetime('now', '-100 hours')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        let deleted = store.cleanup(72).unwrap();
        assert_eq!(deleted, 1);

        let history = store.load_history(&key, 10).unwrap();
        assert!(history.is_empty());
    }
}
