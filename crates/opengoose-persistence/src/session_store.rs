use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Integer, Nullable, Text};
use serde::Serialize;
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
    pub estimated_token_count: i64,
    pub active_session_count: i64,
    pub average_session_duration_seconds: f64,
}

/// Session metric details for a single stored session.
#[derive(Debug, Clone)]
pub struct SessionMetricItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub estimated_token_count: i64,
    pub duration_seconds: i64,
    pub active: bool,
}

/// A conversation message stored in the database.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

/// Export payload for a single stored session and its full message history.
#[derive(Debug, Clone, Serialize)]
pub struct SessionExport {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub messages: Vec<HistoryMessage>,
}

/// Query filters for batch session export.
#[derive(Debug, Clone)]
pub struct SessionExportQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: i64,
}

impl Default for SessionExportQuery {
    fn default() -> Self {
        Self {
            since: None,
            until: None,
            limit: 100,
        }
    }
}

/// Render a single session export as Markdown.
pub fn render_session_export_markdown(export: &SessionExport) -> String {
    let mut markdown = String::new();
    writeln!(&mut markdown, "# OpenGoose Session Export").expect("write to string");
    writeln!(&mut markdown).expect("write to string");
    write_session_details(&mut markdown, export, "##");

    markdown
}

/// Render multiple session exports as Markdown.
pub fn render_batch_session_exports_markdown(
    exports: &[SessionExport],
    since: Option<&str>,
    until: Option<&str>,
) -> String {
    let mut markdown = String::new();
    writeln!(&mut markdown, "# OpenGoose Session Batch Export").expect("write to string");
    writeln!(&mut markdown).expect("write to string");
    writeln!(&mut markdown, "- Sessions: {}", exports.len()).expect("write to string");
    if let Some(since) = since {
        writeln!(&mut markdown, "- Since: {since}").expect("write to string");
    }
    if let Some(until) = until {
        writeln!(&mut markdown, "- Until: {until}").expect("write to string");
    }
    writeln!(&mut markdown).expect("write to string");

    if exports.is_empty() {
        writeln!(&mut markdown, "_No sessions matched the requested range._")
            .expect("write to string");
        return markdown;
    }

    for export in exports {
        writeln!(&mut markdown, "## Session `{}`", export.session_key).expect("write to string");
        writeln!(&mut markdown).expect("write to string");
        write_session_details(&mut markdown, export, "###");
    }

    markdown
}

fn write_session_details(markdown: &mut String, export: &SessionExport, message_heading: &str) {
    writeln!(markdown, "- Session key: `{}`", export.session_key).expect("write to string");
    writeln!(
        markdown,
        "- Active team: {}",
        export.active_team.as_deref().unwrap_or("-")
    )
    .expect("write to string");
    writeln!(markdown, "- Created at: {}", export.created_at).expect("write to string");
    writeln!(markdown, "- Updated at: {}", export.updated_at).expect("write to string");
    writeln!(markdown, "- Message count: {}", export.message_count).expect("write to string");
    writeln!(markdown).expect("write to string");

    if export.messages.is_empty() {
        writeln!(markdown, "_No messages stored for this session._").expect("write to string");
        return;
    }

    for (index, message) in export.messages.iter().enumerate() {
        writeln!(
            markdown,
            "{message_heading} {}. {}",
            index + 1,
            message_heading_text(message)
        )
        .expect("write to string");
        writeln!(markdown).expect("write to string");
        writeln!(markdown, "```text").expect("write to string");
        writeln!(markdown, "{}", message.content).expect("write to string");
        writeln!(markdown, "```").expect("write to string");
        writeln!(markdown).expect("write to string");
    }
}

fn message_heading_text(message: &HistoryMessage) -> String {
    match message.author.as_deref() {
        Some(author) => format!("{} · {} · {}", message.role, author, message.created_at),
        None => format!("{} · {}", message.role, message.created_at),
    }
}

/// Session and conversation history operations on a shared Database.
pub struct SessionStore {
    db: Arc<Database>,
}

#[derive(QueryableByName)]
struct SessionStatsRow {
    #[diesel(sql_type = BigInt)]
    session_count: i64,
    #[diesel(sql_type = BigInt)]
    message_count: i64,
    #[diesel(sql_type = BigInt)]
    estimated_token_count: i64,
    #[diesel(sql_type = BigInt)]
    active_session_count: i64,
    #[diesel(sql_type = Double)]
    average_session_duration_seconds: f64,
}

#[derive(QueryableByName)]
struct SessionMetricRow {
    #[diesel(sql_type = Text)]
    session_key: String,
    #[diesel(sql_type = Nullable<Text>)]
    active_team: Option<String>,
    #[diesel(sql_type = Text)]
    created_at: String,
    #[diesel(sql_type = Text)]
    updated_at: String,
    #[diesel(sql_type = BigInt)]
    message_count: i64,
    #[diesel(sql_type = BigInt)]
    estimated_token_count: i64,
    #[diesel(sql_type = BigInt)]
    duration_seconds: i64,
    #[diesel(sql_type = Integer)]
    active: i32,
}

pub const DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES: i64 = 30;

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

    fn load_history_for_stable_id(
        &self,
        session_key: &str,
        limit: Option<i64>,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        self.db.with(|conn| {
            let rows = match limit {
                Some(limit) => messages::table
                    .filter(messages::session_key.eq(session_key))
                    .order((messages::created_at.desc(), messages::id.desc()))
                    .limit(limit)
                    .select((
                        messages::role,
                        messages::content,
                        messages::author,
                        messages::created_at,
                    ))
                    .load::<(String, String, Option<String>, String)>(conn)?,
                None => messages::table
                    .filter(messages::session_key.eq(session_key))
                    .order((messages::created_at.desc(), messages::id.desc()))
                    .select((
                        messages::role,
                        messages::content,
                        messages::author,
                        messages::created_at,
                    ))
                    .load::<(String, String, Option<String>, String)>(conn)?,
            };

            let mut messages = rows
                .into_iter()
                .map(|(role, content, author, created_at)| HistoryMessage {
                    role,
                    content,
                    author,
                    created_at,
                })
                .collect::<Vec<_>>();
            messages.reverse();
            Ok(messages)
        })
    }

    /// Load the most recent messages for a session.
    pub fn load_history(
        &self,
        key: &SessionKey,
        limit: usize,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        self.load_history_for_stable_id(&key.to_stable_id(), Some(limit as i64))
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

    /// Load a single session export including all persisted messages.
    pub fn export_session(&self, key: &SessionKey) -> PersistenceResult<Option<SessionExport>> {
        let key_str = key.to_stable_id();
        let row = self.db.with(|conn| {
            sessions::table
                .filter(sessions::session_key.eq(&key_str))
                .select((
                    sessions::session_key,
                    sessions::active_team,
                    sessions::created_at,
                    sessions::updated_at,
                ))
                .first::<(String, Option<String>, String, String)>(conn)
                .optional()
                .map_err(Into::into)
        })?;

        let Some((session_key, active_team, created_at, updated_at)) = row else {
            return Ok(None);
        };

        let messages = self.load_history_for_stable_id(&session_key, None)?;
        Ok(Some(SessionExport {
            session_key,
            active_team,
            created_at,
            updated_at,
            message_count: messages.len() as i64,
            messages,
        }))
    }

    /// Load multiple session exports filtered by session activity window.
    pub fn export_sessions(
        &self,
        query: &SessionExportQuery,
    ) -> PersistenceResult<Vec<SessionExport>> {
        let rows = self.db.with(|conn| {
            let mut statement = sessions::table.into_boxed();

            if let Some(since) = query.since.as_deref() {
                statement = statement.filter(sessions::updated_at.ge(since));
            }
            if let Some(until) = query.until.as_deref() {
                statement = statement.filter(sessions::updated_at.le(until));
            }

            statement
                .order(sessions::updated_at.desc())
                .limit(query.limit)
                .select((
                    sessions::session_key,
                    sessions::active_team,
                    sessions::created_at,
                    sessions::updated_at,
                ))
                .load::<(String, Option<String>, String, String)>(conn)
                .map_err(Into::into)
        })?;

        let mut exports = Vec::with_capacity(rows.len());
        for (session_key, active_team, created_at, updated_at) in rows {
            let messages = self.load_history_for_stable_id(&session_key, None)?;
            exports.push(SessionExport {
                session_key,
                active_team,
                created_at,
                updated_at,
                message_count: messages.len() as i64,
                messages,
            });
        }

        Ok(exports)
    }

    /// Return aggregate statistics (session count and message count).
    pub fn stats(&self) -> PersistenceResult<SessionStats> {
        self.db.with(|conn| {
            let window = format!("-{} minutes", DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES);
            let row = diesel::sql_query(
                "SELECT
                    COUNT(*) AS session_count,
                    COALESCE(SUM(message_count), 0) AS message_count,
                    COALESCE(SUM(estimated_token_count), 0) AS estimated_token_count,
                    COALESCE(SUM(active_session_count), 0) AS active_session_count,
                    COALESCE(AVG(duration_seconds), 0.0) AS average_session_duration_seconds
                 FROM (
                    SELECT
                        s.session_key AS session_key,
                        COUNT(m.id) AS message_count,
                        COALESCE(SUM((LENGTH(m.content) + 3) / 4), 0) AS estimated_token_count,
                        CAST(ROUND((julianday(s.updated_at) - julianday(s.created_at)) * 86400.0) AS INTEGER) AS duration_seconds,
                        CASE
                            WHEN s.updated_at >= datetime('now', ?1) THEN 1
                            ELSE 0
                        END AS active_session_count
                    FROM sessions s
                    LEFT JOIN messages m ON m.session_key = s.session_key
                    GROUP BY s.session_key, s.created_at, s.updated_at
                 ) session_metrics",
            )
            .bind::<Text, _>(&window)
            .get_result::<SessionStatsRow>(conn)?;

            Ok(SessionStats {
                session_count: row.session_count,
                message_count: row.message_count,
                estimated_token_count: row.estimated_token_count,
                active_session_count: row.active_session_count,
                average_session_duration_seconds: row.average_session_duration_seconds,
            })
        })
    }

    /// List per-session metrics ordered by most recently updated session first.
    ///
    /// Token usage is estimated using a coarse `~4 chars/token` heuristic because
    /// persisted message rows do not currently store model-native token counts.
    pub fn list_session_metrics(&self, limit: i64) -> PersistenceResult<Vec<SessionMetricItem>> {
        self.db.with(|conn| {
            let window = format!("-{} minutes", DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES);
            let rows = diesel::sql_query(
                "SELECT
                    s.session_key AS session_key,
                    s.active_team AS active_team,
                    s.created_at AS created_at,
                    s.updated_at AS updated_at,
                    COUNT(m.id) AS message_count,
                    COALESCE(SUM((LENGTH(m.content) + 3) / 4), 0) AS estimated_token_count,
                    CAST(ROUND((julianday(s.updated_at) - julianday(s.created_at)) * 86400.0) AS INTEGER) AS duration_seconds,
                    CASE
                        WHEN s.updated_at >= datetime('now', ?1) THEN 1
                        ELSE 0
                    END AS active
                 FROM sessions s
                 LEFT JOIN messages m ON m.session_key = s.session_key
                 GROUP BY s.session_key, s.active_team, s.created_at, s.updated_at
                 ORDER BY s.updated_at DESC
                 LIMIT ?2",
            )
            .bind::<Text, _>(&window)
            .bind::<BigInt, _>(limit)
            .load::<SessionMetricRow>(conn)?;

            Ok(rows
                .into_iter()
                .map(|row| SessionMetricItem {
                    session_key: row.session_key,
                    active_team: row.active_team,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    message_count: row.message_count,
                    estimated_token_count: row.estimated_token_count,
                    duration_seconds: row.duration_seconds.max(0),
                    active: row.active != 0,
                })
                .collect())
        })
    }

    /// Delete individual messages older than the given retention window.
    pub fn cleanup_expired_messages(&self, retention_days: u32) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            let cutoff = format!("-{retention_days} days");
            let deleted =
                diesel::sql_query("DELETE FROM messages WHERE created_at < datetime('now', ?1)")
                    .bind::<Text, _>(&cutoff)
                    .execute(conn)?;
            if deleted > 0 {
                info!(deleted, retention_days, "cleaned up expired messages");
            }
            Ok(deleted)
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
    fn test_cleanup_expired_messages() {
        let db = test_db();
        let store = SessionStore::new(db.clone());
        let key = test_key();

        store.append_user_message(&key, "old msg", None).unwrap();
        store.append_user_message(&key, "recent msg", None).unwrap();

        db.with(|conn| {
            diesel::sql_query(
                "UPDATE messages
                 SET created_at = datetime('now', '-10 days')
                 WHERE content = 'old msg'",
            )
            .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let deleted = store.cleanup_expired_messages(7).unwrap();
        assert_eq!(deleted, 1);

        let history = store.load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "recent msg");
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
    fn test_export_session_includes_metadata_and_all_messages() {
        let store = SessionStore::new(test_db());
        let key = test_key();

        store
            .append_user_message(&key, "hello", Some("alice"))
            .unwrap();
        store.append_assistant_message(&key, "world").unwrap();
        store.set_active_team(&key, Some("feature-dev")).unwrap();

        let export = store
            .export_session(&key)
            .unwrap()
            .expect("session export should exist");

        assert_eq!(export.session_key, "discord:ns:guild123:channel456");
        assert_eq!(export.active_team.as_deref(), Some("feature-dev"));
        assert_eq!(export.message_count, 2);
        assert_eq!(export.messages[0].content, "hello");
        assert_eq!(export.messages[1].content, "world");
    }

    #[test]
    fn test_export_session_returns_none_for_missing_session() {
        let store = SessionStore::new(test_db());
        let missing = SessionKey::new(Platform::Slack, "missing".to_string(), "session");

        let export = store.export_session(&missing).unwrap();

        assert!(export.is_none());
    }

    #[test]
    fn test_export_sessions_filters_by_updated_at_range() {
        let db = test_db();
        let store = SessionStore::new(db.clone());
        let older_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "alpha");
        let newer_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "beta");

        store
            .append_user_message(&older_key, "older", Some("alice"))
            .unwrap();
        store
            .append_user_message(&newer_key, "newer", Some("bob"))
            .unwrap();

        db.with(|conn| {
            diesel::sql_query(
                "UPDATE sessions
                 SET updated_at = '2026-03-09 08:00:00'
                 WHERE session_key = 'discord:ns:guild-a:alpha'",
            )
            .execute(conn)?;
            diesel::sql_query(
                "UPDATE sessions
                 SET updated_at = '2026-03-11 09:00:00'
                 WHERE session_key = 'discord:ns:guild-b:beta'",
            )
            .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let exports = store
            .export_sessions(&SessionExportQuery {
                since: Some("2026-03-10 00:00:00".into()),
                until: Some("2026-03-11 23:59:59".into()),
                limit: 10,
            })
            .unwrap();

        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].session_key, "discord:ns:guild-b:beta");
    }

    #[test]
    fn test_render_session_export_markdown_contains_metadata_and_message_content() {
        let markdown = render_session_export_markdown(&SessionExport {
            session_key: "discord:ns:guild123:channel456".into(),
            active_team: Some("feature-dev".into()),
            created_at: "2026-03-11 09:00:00".into(),
            updated_at: "2026-03-11 09:05:00".into(),
            message_count: 1,
            messages: vec![HistoryMessage {
                role: "user".into(),
                content: "hello markdown".into(),
                author: Some("alice".into()),
                created_at: "2026-03-11 09:01:00".into(),
            }],
        });

        assert!(markdown.contains("# OpenGoose Session Export"));
        assert!(markdown.contains("feature-dev"));
        assert!(markdown.contains("hello markdown"));
    }

    #[test]
    fn test_render_batch_session_exports_markdown_reports_empty_range() {
        let markdown = render_batch_session_exports_markdown(
            &[],
            Some("2026-03-10 00:00:00"),
            Some("2026-03-11 00:00:00"),
        );

        assert!(markdown.contains("# OpenGoose Session Batch Export"));
        assert!(markdown.contains("No sessions matched"));
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

    #[test]
    fn test_stats_include_active_sessions_duration_and_tokens() {
        let db = test_db();
        let store = SessionStore::new(db.clone());
        let active_key = SessionKey::new(Platform::Discord, "guild-a".to_string(), "chan-a");
        let idle_key = SessionKey::new(Platform::Discord, "guild-b".to_string(), "chan-b");

        store
            .append_user_message(&active_key, "12345678", Some("alice"))
            .unwrap();
        store.append_assistant_message(&active_key, "1234").unwrap();
        store
            .append_user_message(&idle_key, "123456789012", Some("bob"))
            .unwrap();

        db.with(|conn| {
            diesel::sql_query(
                "UPDATE sessions
                 SET created_at = datetime('now', '-90 seconds'),
                     updated_at = datetime('now')
                 WHERE session_key = 'discord:ns:guild-a:chan-a'",
            )
            .execute(conn)?;
            diesel::sql_query(
                "UPDATE sessions
                 SET created_at = datetime('now', '-2 hours', '-30 seconds'),
                     updated_at = datetime('now', '-2 hours')
                 WHERE session_key = 'discord:ns:guild-b:chan-b'",
            )
            .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 2);
        assert_eq!(stats.message_count, 3);
        assert_eq!(stats.estimated_token_count, 6);
        assert_eq!(stats.active_session_count, 1);
        assert!((stats.average_session_duration_seconds - 60.0).abs() < 1.0);
    }

    #[test]
    fn test_list_session_metrics_returns_recent_breakdown() {
        let db = test_db();
        let store = SessionStore::new(db.clone());
        let active_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "alpha");
        let idle_key = SessionKey::new(Platform::Slack, "workspace".to_string(), "beta");

        store
            .append_user_message(&active_key, "abcdefgh", Some("alice"))
            .unwrap();
        store
            .append_assistant_message(&active_key, "ijklmnop")
            .unwrap();
        store.set_active_team(&active_key, Some("ops")).unwrap();
        store
            .append_user_message(&idle_key, "abcdefghij", Some("bob"))
            .unwrap();

        db.with(|conn| {
            diesel::sql_query(
                "UPDATE sessions
                 SET created_at = datetime('now', '-120 seconds'),
                     updated_at = datetime('now')
                 WHERE session_key = 'slack:ns:workspace:alpha'",
            )
            .execute(conn)?;
            diesel::sql_query(
                "UPDATE sessions
                 SET created_at = datetime('now', '-3 hours', '-45 seconds'),
                     updated_at = datetime('now', '-3 hours')
                 WHERE session_key = 'slack:ns:workspace:beta'",
            )
            .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let metrics = store.list_session_metrics(10).unwrap();
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].session_key, "slack:ns:workspace:alpha");
        assert_eq!(metrics[0].message_count, 2);
        assert_eq!(metrics[0].estimated_token_count, 4);
        assert_eq!(metrics[0].duration_seconds, 120);
        assert!(metrics[0].active);
        assert_eq!(metrics[0].active_team.as_deref(), Some("ops"));

        assert_eq!(metrics[1].session_key, "slack:ns:workspace:beta");
        assert_eq!(metrics[1].message_count, 1);
        assert_eq!(metrics[1].estimated_token_count, 3);
        assert_eq!(metrics[1].duration_seconds, 45);
        assert!(!metrics[1].active);
    }
}
