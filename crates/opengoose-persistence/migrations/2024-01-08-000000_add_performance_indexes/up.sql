-- Optimise list_due() which filters by enabled=1 AND next_run_at <= now().
CREATE INDEX IF NOT EXISTS idx_schedules_enabled_next_run
    ON schedules(enabled, next_run_at);

-- Optimise enabled trigger lookups by type (used in trigger evaluation loops).
CREATE INDEX IF NOT EXISTS idx_triggers_enabled_type
    ON triggers(enabled, trigger_type);
