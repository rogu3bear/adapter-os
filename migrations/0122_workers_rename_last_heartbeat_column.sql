-- Rename last_heartbeat_at to last_seen_at for consistency with code
-- The Worker struct and all queries use last_seen_at but the table has last_heartbeat_at

ALTER TABLE workers RENAME COLUMN last_heartbeat_at TO last_seen_at;

