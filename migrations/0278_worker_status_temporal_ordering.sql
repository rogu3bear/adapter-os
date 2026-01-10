-- Migration: Enforce temporal ordering for worker status history
-- Purpose: Prevent out-of-order status history entries

-- Create a trigger to validate temporal ordering
-- New entries must have created_at > max(created_at) for the same worker
-- Note: Uses > instead of >= to allow same-second inserts (SQLite datetime has second precision)
CREATE TRIGGER IF NOT EXISTS enforce_worker_status_temporal_ordering
BEFORE INSERT ON worker_status_history
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM worker_status_history
    WHERE worker_id = NEW.worker_id
      AND created_at > NEW.created_at
)
BEGIN
    SELECT RAISE(ABORT, 'Worker status history must be temporally ordered: new entry timestamp must not be before existing entries');
END;

-- Add index to optimize temporal ordering checks
CREATE INDEX IF NOT EXISTS idx_worker_status_history_temporal
ON worker_status_history(worker_id, created_at DESC);
