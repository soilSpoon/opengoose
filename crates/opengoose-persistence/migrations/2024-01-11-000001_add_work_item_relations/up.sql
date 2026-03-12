CREATE TABLE work_item_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_item_id INTEGER NOT NULL REFERENCES work_items(id),
    to_item_id INTEGER NOT NULL REFERENCES work_items(id),
    relation_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(from_item_id, to_item_id, relation_type)
);

CREATE INDEX idx_relations_from ON work_item_relations(from_item_id);
CREATE INDEX idx_relations_to ON work_item_relations(to_item_id);
