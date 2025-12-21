-- Query Performance Optimization
-- Migration: 0081
-- Created: 2025-11-21
-- Purpose: Add critical indexes for frequently queried columns to improve query performance

-- Index for user email lookups (used by get_user_by_username)
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Index for adapter lookups by name and tenant (used by list_adapters_by_tenant)
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_name ON adapters(tenant_id, name);

-- Index for adapter lifecycle state queries (used by lifecycle management)
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state ON adapters(tenant_id, lifecycle_state);

-- Composite index for adapter eviction queries (load_state + last_activated)
CREATE INDEX IF NOT EXISTS idx_adapters_eviction ON adapters(load_state, last_activated);

-- Index for adapter activation tracking
CREATE INDEX IF NOT EXISTS idx_adapters_activation ON adapters(tenant_id, activation_count DESC);

-- Index for adapter expiration queries (TTL enforcement)
CREATE INDEX IF NOT EXISTS idx_adapters_expires ON adapters(expires_at) WHERE expires_at IS NOT NULL;

-- Index for pinned adapters queries
CREATE INDEX IF NOT EXISTS idx_adapters_pinned ON adapters(tenant_id, pinned) WHERE pinned = 1;

-- Index for tier-based queries (tier selection and lifecycle promotion)
CREATE INDEX IF NOT EXISTS idx_adapters_tier ON adapters(tenant_id, tier);

-- Index for adapter stack queries by tenant
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_tenant ON adapter_stacks(tenant_id);

-- Index for audit log queries by action
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action, status);

-- Index for user role-based access control queries
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

-- Index for tenant-scoped user queries
CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id) WHERE tenant_id IS NOT NULL;

-- Index for routing decision queries (using stack_id, adapter_id doesn't exist)
CREATE INDEX IF NOT EXISTS idx_routing_decisions_stack ON routing_decisions(stack_id) WHERE stack_id IS NOT NULL;

-- Index for routing history timeline queries
CREATE INDEX IF NOT EXISTS idx_routing_decisions_timestamp ON routing_decisions(timestamp DESC);

-- Composite index for cache-friendly adapter lookups
CREATE INDEX IF NOT EXISTS idx_adapters_lookup ON adapters(tenant_id, adapter_id, active);

-- Analyze tables to update query planner statistics
ANALYZE adapters;
ANALYZE users;
ANALYZE adapter_stacks;
ANALYZE audit_logs;
ANALYZE routing_decisions;
