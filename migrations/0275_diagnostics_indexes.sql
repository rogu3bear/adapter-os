-- Add missing composite indexes for diag_runs table
-- Evidence: Query Performance Optimization
--
-- Addresses queries with patterns:
--   WHERE tenant_id = ? AND started_at_unix_ms >= ?
--   WHERE tenant_id = ? AND status = ?
--
-- These composite indexes enable efficient range scans and status filtering
-- scoped to a specific tenant without full table scans.

-- Composite index for time-range queries per tenant
-- Supports: ORDER BY started_at_unix_ms, range filters on started_at_unix_ms
CREATE INDEX IF NOT EXISTS idx_diag_runs_tenant_started
    ON diag_runs(tenant_id, started_at_unix_ms);

-- Composite index for status filtering per tenant
-- Supports: WHERE tenant_id = ? AND status = ?
CREATE INDEX IF NOT EXISTS idx_diag_runs_tenant_status
    ON diag_runs(tenant_id, status);
