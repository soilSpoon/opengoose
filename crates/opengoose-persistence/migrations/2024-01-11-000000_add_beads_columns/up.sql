-- Add Beads-inspired columns to work_items
ALTER TABLE work_items ADD COLUMN hash_id TEXT;
ALTER TABLE work_items ADD COLUMN is_ephemeral INTEGER NOT NULL DEFAULT 0;
ALTER TABLE work_items ADD COLUMN priority INTEGER NOT NULL DEFAULT 3;
CREATE UNIQUE INDEX idx_work_items_hash_id ON work_items(hash_id) WHERE hash_id IS NOT NULL;

-- Wisp digest storage (summaries of burned/squashed wisps)
CREATE TABLE wisp_digests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    original_wisp_id INTEGER NOT NULL,
    agent_name TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Compacted work item summaries
CREATE TABLE work_item_compacted (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_run_id TEXT NOT NULL,
    parent_id INTEGER,
    summary TEXT NOT NULL,
    item_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
