-- Rollback for Migration 0210: Tenant-Scoped Query Performance Optimization
-- Optimization-ID: dbopt-0210-tenant-scoped-composite-indexes
-- Purpose: Remove composite indexes added for tenant-scoped hot queries.
-- Warning: Dropping indexes may degrade performance; use only during incident response.

DROP INDEX IF EXISTS idx_adapters_tenant_active_tier_created;
DROP INDEX IF EXISTS idx_adapters_tenant_hash_active;
DROP INDEX IF EXISTS idx_adapters_tenant_expires;
DROP INDEX IF EXISTS idx_documents_tenant_created;
DROP INDEX IF EXISTS idx_training_jobs_tenant_status_created;
DROP INDEX IF EXISTS idx_chat_messages_tenant_created;
DROP INDEX IF EXISTS idx_base_model_status_tenant_model;

-- Refresh planner statistics after index changes
ANALYZE adapters;
ANALYZE documents;
ANALYZE repository_training_jobs;
ANALYZE chat_messages;
ANALYZE base_model_status;
ANALYZE routing_decisions;

