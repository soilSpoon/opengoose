CREATE TABLE schedules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name        TEXT    NOT NULL UNIQUE,
    cron_expression TEXT NOT NULL,
    team_name   TEXT    NOT NULL,
    input       TEXT    NOT NULL DEFAULT '',
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    next_run_at TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
