DROP TABLE IF EXISTS work_item_compacted;
DROP TABLE IF EXISTS wisp_digests;
-- SQLite does not support DROP COLUMN before 3.35.0; recreate table if needed.
-- For simplicity, we leave the columns in place on down migration.
