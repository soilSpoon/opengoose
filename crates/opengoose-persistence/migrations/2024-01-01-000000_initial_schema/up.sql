-- Sessions and conversation history
CREATE TABLE sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key TEXT NOT NULL UNIQUE,
    active_team TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    author      TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_messages_session
    ON messages(session_key, created_at);

-- Agent-to-agent message queue
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

CREATE INDEX idx_mq_recipient_status
    ON message_queue(recipient, status);
CREATE INDEX idx_mq_team_run
    ON message_queue(team_run_id, created_at);

-- Work items
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

CREATE INDEX idx_wi_session
    ON work_items(session_key, status);
CREATE INDEX idx_wi_parent
    ON work_items(parent_id);
CREATE INDEX idx_wi_team_run
    ON work_items(team_run_id);

-- Orchestration run tracking
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

CREATE INDEX idx_or_session
    ON orchestration_runs(session_key, status);
