-- Reverse migration: remove FK constraints and revert work_items PK to TEXT.
-- We recreate all affected tables without FK constraints.

PRAGMA foreign_keys = OFF;

-- ── messages ──────────────────────────────────────────────────
ALTER TABLE messages RENAME TO _messages_old;

CREATE TABLE messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key TEXT NOT NULL,
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
    session_key  TEXT NOT NULL,
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

-- ── work_items (revert PK to TEXT) ────────────────────────────
ALTER TABLE work_items RENAME TO _work_items_old;

CREATE TABLE work_items (
    id            TEXT PRIMARY KEY NOT NULL,
    session_key   TEXT NOT NULL,
    team_run_id   TEXT NOT NULL,
    parent_id     TEXT,
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

INSERT INTO work_items (id, session_key, team_run_id, title, description,
                        status, assigned_to, workflow_step, input,
                        output, error, created_at, updated_at)
    SELECT CAST(id AS TEXT), session_key, team_run_id, title, description,
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
    session_key  TEXT NOT NULL,
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
    session_key   TEXT,
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
