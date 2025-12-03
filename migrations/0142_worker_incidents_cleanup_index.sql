-- Migration 0142: Add index for efficient worker incidents cleanup
-- Purpose: Support retention cleanup queries on worker_incidents table
-- PRD: PRD-06 (Retention policies, audit trail management)
-- Created: 2025-12-03

-- Index for efficient cleanup queries: DELETE WHERE created_at < ?
-- Without this index, retention cleanup would scan entire table.
CREATE INDEX IF NOT EXISTS idx_worker_incidents_created_at
ON worker_incidents(created_at);

-- Update query planner statistics after index creation
ANALYZE worker_incidents;
