-- Add platform prefix to existing session keys for multi-channel support.
-- Legacy keys (without platform prefix) are assumed to be Discord.
--
-- Disable FK enforcement while rewriting keys so that child-table updates
-- do not fail referential integrity checks against the parent (sessions)
-- table whose keys are also being rewritten in the same migration.
PRAGMA foreign_keys = OFF;

-- Update parent table first
UPDATE sessions SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

-- Then child tables
UPDATE messages SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE message_queue SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE work_items SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE orchestration_runs SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

PRAGMA foreign_keys = ON;
