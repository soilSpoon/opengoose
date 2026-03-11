use std::collections::HashMap;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Text};

use opengoose_types::SessionKey;

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::schema::{messages, sessions};

use super::types::{
    HistoryMessage, SessionExport, SessionExportQuery, SessionItem, SessionMetricItem,
    SessionMetricRow, SessionStats, SessionStatsRow,
};

pub const DEFAULT_ACTIVE_SESSION_WINDOW_MINUTES: i64 = 30;

/// Session query (read) operations on a shared Database.
pub(super) struct SessionQueries;

impl SessionQueries {
    pub fn load_history_for_stable_id(
        db: &Arc<Database>,
        session_key: &str,
        limit: Option<i64>,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        db.with(|conn| {
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

    pub fn load_history(
        db: &Arc<Database>,
        key: &SessionKey,
        limit: usize,
    ) -> PersistenceResult<Vec<HistoryMessage>> {
        Self::load_history_for_stable_id(db, &key.to_stable_id(), Some(limit as i64))
    }

    pub fn get_active_team(
        db: &Arc<Database>,
        key: &SessionKey,
    ) -> PersistenceResult<Option<String>> {
        db.with(|conn| {
            let key_str = key.to_stable_id();
            let result = sessions::table
                .filter(sessions::session_key.eq(&key_str))
                .select(sessions::active_team)
                .first::<Option<String>>(conn)
                .optional()?;
            Ok(result.flatten())
        })
    }

    pub fn load_all_active_teams(
        db: &Arc<Database>,
    ) -> PersistenceResult<HashMap<SessionKey, String>> {
        db.with(|conn| {
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

    pub fn list_sessions(db: &Arc<Database>, limit: i64) -> PersistenceResult<Vec<SessionItem>> {
        db.with(|conn| {
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

    pub fn export_session(
        db: &Arc<Database>,
        key: &SessionKey,
    ) -> PersistenceResult<Option<SessionExport>> {
        let key_str = key.to_stable_id();
        let row = db.with(|conn| {
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

        let messages = Self::load_history_for_stable_id(db, &session_key, None)?;
        Ok(Some(SessionExport {
            session_key,
            active_team,
            created_at,
            updated_at,
            message_count: messages.len() as i64,
            messages,
        }))
    }

    pub fn export_sessions(
        db: &Arc<Database>,
        query: &SessionExportQuery,
    ) -> PersistenceResult<Vec<SessionExport>> {
        let rows = db.with(|conn| {
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
            let messages = Self::load_history_for_stable_id(db, &session_key, None)?;
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

    pub fn stats(db: &Arc<Database>) -> PersistenceResult<SessionStats> {
        db.with(|conn| {
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

    pub fn list_session_metrics(
        db: &Arc<Database>,
        limit: i64,
    ) -> PersistenceResult<Vec<SessionMetricItem>> {
        db.with(|conn| {
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
}
