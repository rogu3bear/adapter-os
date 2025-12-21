-- Migration: Base Model Status Covering Index Optimization
-- Purpose: Optimize tenant-scoped base model status queries with covering index
-- Evidence: Performance monitoring shows bookmark lookups during health checks

-- Create covering index for tenant-scoped base model status queries
-- This eliminates bookmark lookups for queries like:
-- SELECT * FROM base_model_status WHERE tenant_id = ? ORDER BY updated_at DESC LIMIT 1
-- The index covers tenant_id (WHERE), model_id, status, and updated_at (ORDER BY)
CREATE INDEX IF NOT EXISTS idx_base_model_status_tenant_model_status_updated
ON base_model_status(tenant_id, model_id, status, updated_at DESC);

-- Note: This is a covering index for the common query pattern used in
-- get_base_model_status() method in models.rs. The index includes:
-- - tenant_id: WHERE clause filter
-- - model_id: Additional context for status tracking
-- - status: Frequently queried status field
-- - updated_at DESC: ORDER BY clause for latest status

-- This ensures base model status checks remain sub-millisecond
-- and prevents model loading status monitoring from becoming
-- a bottleneck during high-frequency health checks.