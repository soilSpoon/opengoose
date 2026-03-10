CREATE TABLE plugins (
    id              INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name            TEXT    NOT NULL UNIQUE,
    version         TEXT    NOT NULL,
    author          TEXT,
    description     TEXT,
    capabilities    TEXT    NOT NULL DEFAULT '',
    source_path     TEXT    NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);
