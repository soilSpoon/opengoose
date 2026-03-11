PRAGMA foreign_keys = OFF;

CREATE TABLE sessions__new (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key TEXT NOT NULL UNIQUE,
    active_team TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO sessions__new (id, session_key, active_team, created_at, updated_at)
SELECT id, session_key, active_team, created_at, updated_at
FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions__new RENAME TO sessions;

PRAGMA foreign_keys = ON;
