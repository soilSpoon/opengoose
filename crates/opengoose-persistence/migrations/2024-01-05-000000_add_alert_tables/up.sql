-- Alert rules: threshold-based conditions on system health metrics
CREATE TABLE alert_rules (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    metric      TEXT NOT NULL, -- 'queue_backlog', 'error_rate', 'failed_runs'
    condition   TEXT NOT NULL, -- 'gt', 'lt', 'gte', 'lte'
    threshold   REAL NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Alert history: record of every time a rule was triggered
CREATE TABLE alert_history (
    id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    rule_id      TEXT NOT NULL,
    rule_name    TEXT NOT NULL,
    metric       TEXT NOT NULL,
    value        REAL NOT NULL,
    triggered_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_alert_history_rule
    ON alert_history(rule_id, triggered_at);
