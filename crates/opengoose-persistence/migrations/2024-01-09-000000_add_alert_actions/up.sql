-- Add actions column to alert_rules for notification dispatch (Webhook, Log, ChannelMessage)
ALTER TABLE alert_rules ADD COLUMN actions TEXT NOT NULL DEFAULT '[]';
