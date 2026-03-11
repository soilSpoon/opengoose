use std::sync::Arc;

use diesel::prelude::*;
use tracing::{debug, info};

use diesel::sql_types::Text;
use opengoose_types::SessionKey;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewMessage, NewSession};
use crate::schema::{messages, sessions};

/// Session mutation (write) operations on a shared Database.
pub(super) struct SessionMutations;

impl SessionMutations {
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
    pub fn append_message(
        db: &Arc<Database>,
        key: &SessionKey,
        role: &str,
        content: &str,
        author: Option<&str>,
    ) -> PersistenceResult<()> {
        db.with(|conn| {
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

    /// Set or clear the active team for a session.
    pub fn set_active_team(
        db: &Arc<Database>,
        key: &SessionKey,
        team: Option<&str>,
    ) -> PersistenceResult<()> {
        db.with(|conn| {
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

    /// Delete individual messages older than the given retention window.
    pub fn cleanup_expired_messages(
        db: &Arc<Database>,
        retention_days: u32,
    ) -> PersistenceResult<usize> {
        db.with(|conn| {
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
    pub fn cleanup(db: &Arc<Database>, max_age_hours: i64) -> PersistenceResult<usize> {
        db.with(|conn| {
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
