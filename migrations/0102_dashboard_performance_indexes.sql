-- Dashboard Performance Optimization
-- Migration: 0102
-- Created: 2025-11-26
-- Purpose: Add indexes for dashboard and middleware queries to improve UI responsiveness

-- ============================================================================
-- TENANT MIDDLEWARE VALIDATION
-- ============================================================================
-- Index for fast tenant existence checks in middleware (used by enhanced_auth_middleware)
-- This supports the tenant validation cache miss path
CREATE INDEX IF NOT EXISTS idx_tenants_id_lookup ON tenants(id);

-- ============================================================================
-- SYSTEM OVERVIEW QUERIES
-- ============================================================================
-- Index for active sessions query (chat_sessions without status column)
-- Used by: get_system_overview() counting active sessions by last_activity_at
CREATE INDEX IF NOT EXISTS idx_chat_sessions_last_activity ON chat_sessions(last_activity_at DESC);

-- Index for workers status query
-- Used by: get_system_overview() counting workers by status
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);

-- Index for active adapters count
-- Used by: get_system_overview() counting active adapters
CREATE INDEX IF NOT EXISTS idx_adapters_active ON adapters(active) WHERE active = 1;

-- ============================================================================
-- TOKEN REVOCATION CHECKS
-- ============================================================================
-- Index for token revocation lookups (authentication middleware)
-- Used by: is_token_revoked() checking if a token JTI is revoked
CREATE INDEX IF NOT EXISTS idx_revoked_tokens_jti ON revoked_tokens(jti);

-- Index for user sessions by JTI (session activity updates)
-- Used by: update_session_activity() updating last_activity for sessions
CREATE INDEX IF NOT EXISTS idx_user_sessions_jti ON user_sessions(jti);

-- Composite index for adapter system state queries
-- Used by: get_system_state() batch fetching adapters with tenant filtering
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_active_memory ON adapters(tenant_id, active, memory_bytes DESC);

-- ============================================================================
-- ANALYZE TABLES (only tables that are guaranteed to exist)
-- ============================================================================
-- Update query planner statistics for core tables
ANALYZE tenants;
ANALYZE adapters;
ANALYZE chat_sessions;
ANALYZE workers;
ANALYZE revoked_tokens;
ANALYZE user_sessions;

-- NOTE: Indexes for stack_adapters and pinned_adapters are NOT included
-- because those tables may not exist in all deployments (created in migration 0064).
-- The code handles missing indexes gracefully - they just won't be optimized.
-- Similarly, telemetry_events and routing_decisions indexes are excluded.
