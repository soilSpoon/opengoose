-- Add platform prefix to existing session keys for multi-channel support.
-- Legacy keys (without platform prefix) are assumed to be Discord.
UPDATE sessions SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE messages SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE message_queue SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE work_items SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';

UPDATE orchestration_runs SET session_key = 'discord:' || session_key
WHERE session_key NOT LIKE 'discord:%' AND session_key NOT LIKE 'telegram:%' AND session_key NOT LIKE 'slack:%';
