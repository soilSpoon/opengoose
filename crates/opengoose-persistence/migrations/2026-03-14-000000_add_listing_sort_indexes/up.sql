-- Optimise list_sessions() / export_sessions() which ORDER BY sessions.updated_at DESC.
-- Without this index SQLite does a full-table scan + sort on every dashboard load.
CREATE INDEX IF NOT EXISTS idx_sessions_updated_at
    ON sessions(updated_at DESC);

-- Optimise list_runs() (no status filter) which ORDER BY orchestration_runs.updated_at DESC.
-- dashboard.rs calls list_runs(None, i64::MAX) — an index scan is essential there.
CREATE INDEX IF NOT EXISTS idx_or_updated_at
    ON orchestration_runs(updated_at DESC);

-- Optimise list_runs(Some(status)) which filters by status AND sorts by updated_at DESC.
-- Covers the /api/runs?status=… endpoint in runs.rs.
CREATE INDEX IF NOT EXISTS idx_or_status_updated_at
    ON orchestration_runs(status, updated_at DESC);
