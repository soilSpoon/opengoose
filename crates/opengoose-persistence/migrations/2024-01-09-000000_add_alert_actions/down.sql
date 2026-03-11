-- SQLite does not support DROP COLUMN in older versions; recreate table without actions
CREATE TABLE alert_rules_backup AS SELECT id, name, description, metric, condition, threshold, enabled, created_at, updated_at FROM alert_rules;
DROP TABLE alert_rules;
ALTER TABLE alert_rules_backup RENAME TO alert_rules;
