CREATE TABLE triggers (
    id              INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name            TEXT    NOT NULL UNIQUE,
    trigger_type    TEXT    NOT NULL,
    condition_json  TEXT    NOT NULL DEFAULT '{}',
    team_name       TEXT    NOT NULL,
    input           TEXT    NOT NULL DEFAULT '',
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_fired_at   TEXT,
    fire_count      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);
