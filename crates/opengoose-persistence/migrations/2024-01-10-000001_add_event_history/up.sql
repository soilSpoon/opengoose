CREATE TABLE event_history (
    id             INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    event_kind     TEXT NOT NULL,
    timestamp      TEXT NOT NULL DEFAULT (datetime('now')),
    source_gateway TEXT,
    session_key    TEXT,
    payload        TEXT NOT NULL
);

CREATE INDEX idx_event_history_timestamp
    ON event_history(timestamp DESC);

CREATE INDEX idx_event_history_gateway_timestamp
    ON event_history(source_gateway, timestamp DESC);

CREATE INDEX idx_event_history_session_timestamp
    ON event_history(session_key, timestamp DESC);

CREATE INDEX idx_event_history_kind_timestamp
    ON event_history(event_kind, timestamp DESC);
