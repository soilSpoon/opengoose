-- Reverse: strip 'discord:' prefix from session keys.
-- Only reverses the Discord prefix since Telegram/Slack keys would not have existed before.
UPDATE sessions SET session_key = SUBSTR(session_key, 9)
WHERE session_key LIKE 'discord:%';

UPDATE messages SET session_key = SUBSTR(session_key, 9)
WHERE session_key LIKE 'discord:%';

UPDATE message_queue SET session_key = SUBSTR(session_key, 9)
WHERE session_key LIKE 'discord:%';

UPDATE work_items SET session_key = SUBSTR(session_key, 9)
WHERE session_key LIKE 'discord:%';

UPDATE orchestration_runs SET session_key = SUBSTR(session_key, 9)
WHERE session_key LIKE 'discord:%';
