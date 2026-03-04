use rusqlite::Connection;

use crate::error::PersistenceResult;

pub fn initialize(conn: &Connection) -> PersistenceResult<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        -- Sessions and conversation history
        CREATE TABLE IF NOT EXISTS sessions (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_key TEXT NOT NULL UNIQUE,
            active_team TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_key TEXT NOT NULL,
            role        TEXT NOT NULL,
            content     TEXT NOT NULL,
            author      TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_messages_session
            ON messages(session_key, created_at);

        -- Agent-to-agent message queue (TinyClaw-inspired)
        CREATE TABLE IF NOT EXISTS message_queue (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_key TEXT NOT NULL,
            team_run_id TEXT NOT NULL,
            sender      TEXT NOT NULL,
            recipient   TEXT NOT NULL,
            content     TEXT NOT NULL,
            msg_type    TEXT NOT NULL DEFAULT 'task',
            status      TEXT NOT NULL DEFAULT 'pending',
            retry_count INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 3,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            processed_at TEXT,
            error       TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_mq_recipient_status
            ON message_queue(recipient, status);
        CREATE INDEX IF NOT EXISTS idx_mq_team_run
            ON message_queue(team_run_id, created_at);

        -- Work items (Gas Town Beads-inspired)
        CREATE TABLE IF NOT EXISTS work_items (
            id          TEXT PRIMARY KEY,
            session_key TEXT NOT NULL,
            team_run_id TEXT NOT NULL,
            parent_id   TEXT,
            title       TEXT NOT NULL,
            description TEXT,
            status      TEXT NOT NULL DEFAULT 'pending',
            assigned_to TEXT,
            workflow_step INTEGER,
            input       TEXT,
            output      TEXT,
            error       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_wi_session
            ON work_items(session_key, status);
        CREATE INDEX IF NOT EXISTS idx_wi_parent
            ON work_items(parent_id);
        CREATE INDEX IF NOT EXISTS idx_wi_team_run
            ON work_items(team_run_id);

        -- Orchestration run tracking (crash recovery)
        CREATE TABLE IF NOT EXISTS orchestration_runs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            team_run_id TEXT NOT NULL UNIQUE,
            session_key TEXT NOT NULL,
            team_name   TEXT NOT NULL,
            workflow    TEXT NOT NULL,
            input       TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'running',
            current_step INTEGER NOT NULL DEFAULT 0,
            total_steps  INTEGER NOT NULL DEFAULT 0,
            result      TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_or_session
            ON orchestration_runs(session_key, status);
        ",
    )?;
    Ok(())
}
