CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    key_hash TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT
);

CREATE UNIQUE INDEX idx_api_keys_key_hash ON api_keys(key_hash);
