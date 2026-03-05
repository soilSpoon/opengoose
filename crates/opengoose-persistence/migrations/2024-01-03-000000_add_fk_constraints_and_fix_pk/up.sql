-- ============================================================
-- Migration: add foreign key constraints + fix work_items PK
-- ============================================================
-- SQLite does not support ALTER TABLE … ADD CONSTRAINT, so we
-- recreate tables that need FK constraints.  Tables are handled
-- in dependency order (parents first, children last).
--
-- sessions: no changes needed (root table).
-- messages: add FK on session_key → sessions(session_key).
-- message_queue: add FK on session_key → sessions(session_key).
-- work_items: change PK from TEXT to INTEGER AUTOINCREMENT,
--             add FK on session_key → sessions(session_key),
--             add self-referencing FK on parent_id → work_items(id).
-- orchestration_runs: add FK on session_key → sessions(session_key).
-- workflow_runs: add FK on session_key → sessions(session_key).
-- ============================================================

PRAGMA foreign_keys = OFF;

-- ── messages ──────────────────────────────────────────────────
ALTER TABLE messages RENAME TO _messages_old;

CREATE TABLE messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key TEXT NOT NULL REFERENCES sessions(session_key),
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    author      TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO messages SELECT * FROM _messages_old;
DROP TABLE _messages_old;

CREATE INDEX idx_messages_session
    ON messages(session_key, created_at);

-- ── message_queue ─────────────────────────────────────────────
ALTER TABLE message_queue RENAME TO _message_queue_old;

CREATE TABLE message_queue (
    id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key  TEXT NOT NULL REFERENCES sessions(session_key),
    team_run_id  TEXT NOT NULL,
    sender       TEXT NOT NULL,
    recipient    TEXT NOT NULL,
    content      TEXT NOT NULL,
    msg_type     TEXT NOT NULL DEFAULT 'task',
    status       TEXT NOT NULL DEFAULT 'pending',
    retry_count  INTEGER NOT NULL DEFAULT 0,
    max_retries  INTEGER NOT NULL DEFAULT 3,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    processed_at TEXT,
    error        TEXT
);

INSERT INTO message_queue SELECT * FROM _message_queue_old;
DROP TABLE _message_queue_old;

CREATE INDEX idx_mq_recipient_status
    ON message_queue(recipient, status);
CREATE INDEX idx_mq_team_run
    ON message_queue(team_run_id, created_at);

-- ── work_items (PK TEXT → INTEGER + add FKs) ─────────────────
ALTER TABLE work_items RENAME TO _work_items_old;

CREATE TABLE work_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key   TEXT NOT NULL REFERENCES sessions(session_key),
    team_run_id   TEXT NOT NULL,
    parent_id     INTEGER REFERENCES work_items(id),
    title         TEXT NOT NULL,
    description   TEXT,
    status        TEXT NOT NULL DEFAULT 'pending',
    assigned_to   TEXT,
    workflow_step INTEGER,
    input         TEXT,
    output        TEXT,
    error         TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Old data had TEXT ids; we discard them and let AUTOINCREMENT
-- assign new integer ids.  parent_id linkage is lost for any
-- pre-existing rows, but this is acceptable for a dev migration.
INSERT INTO work_items (session_key, team_run_id, title, description,
                        status, assigned_to, workflow_step, input,
                        output, error, created_at, updated_at)
    SELECT session_key, team_run_id, title, description,
           status, assigned_to, workflow_step, input,
           output, error, created_at, updated_at
    FROM _work_items_old;

DROP TABLE _work_items_old;

CREATE INDEX idx_wi_session
    ON work_items(session_key, status);
CREATE INDEX idx_wi_parent
    ON work_items(parent_id);
CREATE INDEX idx_wi_team_run
    ON work_items(team_run_id);

-- ── orchestration_runs ────────────────────────────────────────
ALTER TABLE orchestration_runs RENAME TO _orchestration_runs_old;

CREATE TABLE orchestration_runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    team_run_id  TEXT NOT NULL UNIQUE,
    session_key  TEXT NOT NULL REFERENCES sessions(session_key),
    team_name    TEXT NOT NULL,
    workflow     TEXT NOT NULL,
    input        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    current_step INTEGER NOT NULL DEFAULT 0,
    total_steps  INTEGER NOT NULL DEFAULT 0,
    result       TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO orchestration_runs SELECT * FROM _orchestration_runs_old;
DROP TABLE _orchestration_runs_old;

CREATE INDEX idx_or_session
    ON orchestration_runs(session_key, status);

-- ── workflow_runs ─────────────────────────────────────────────
ALTER TABLE workflow_runs RENAME TO _workflow_runs_old;

CREATE TABLE workflow_runs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id        TEXT NOT NULL UNIQUE,
    session_key   TEXT REFERENCES sessions(session_key),
    workflow_name TEXT NOT NULL,
    input         TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'running',
    current_step  INTEGER NOT NULL DEFAULT 0,
    total_steps   INTEGER NOT NULL DEFAULT 0,
    state_json    TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO workflow_runs SELECT * FROM _workflow_runs_old;
DROP TABLE _workflow_runs_old;

CREATE INDEX idx_wr_session ON workflow_runs(session_key, status);
CREATE INDEX idx_wr_name ON workflow_runs(workflow_name);

PRAGMA foreign_keys = ON;
