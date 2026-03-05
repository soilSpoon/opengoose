-- Workflow run tracking (parallel to orchestration_runs for teams)
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

CREATE INDEX idx_wr_session ON workflow_runs(session_key, status);
CREATE INDEX idx_wr_name ON workflow_runs(workflow_name);
