CREATE TABLE agent_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    session_key  TEXT    NOT NULL,
    from_agent   TEXT    NOT NULL,
    to_agent     TEXT,           -- NULL means channel/broadcast
    channel      TEXT,           -- NULL means direct message
    payload      TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'pending',  -- pending, delivered, acknowledged
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    delivered_at TEXT
);

CREATE INDEX idx_agent_messages_session ON agent_messages (session_key);
CREATE INDEX idx_agent_messages_to_agent ON agent_messages (to_agent, status);
CREATE INDEX idx_agent_messages_channel ON agent_messages (channel, session_key);
