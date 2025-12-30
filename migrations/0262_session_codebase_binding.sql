-- Migration 0262: Add session-side codebase adapter binding
--
-- Purpose: Complete the bidirectional binding between chat sessions and codebase adapters.
-- - Session side: codebase_adapter_id points to the bound codebase adapter
-- - Adapter side: stream_session_id points to the owning session (from 0261)
--
-- This enables:
-- - Session lookup of its codebase adapter
-- - Enforcement of exclusive binding (one codebase adapter per session)
-- - Clean unbinding when session ends
--
-- Evidence: Codebase Adapters PRD - stream-scoped binding

-- =============================================================================
-- Session-Side Codebase Adapter Binding
-- =============================================================================

-- codebase_adapter_id: The codebase adapter bound to this session
-- Forms bidirectional link with adapters.stream_session_id
-- Note: Using TEXT without FK constraint for SQLite compatibility (adapter_id is not PK)
ALTER TABLE chat_sessions ADD COLUMN codebase_adapter_id TEXT;

-- =============================================================================
-- Indexes for Session Binding
-- =============================================================================

-- Unique constraint: each codebase adapter can only be bound to one session
-- (Adapter-side constraint is in 0261 via stream_session_id unique index)
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_codebase_adapter
    ON chat_sessions(codebase_adapter_id)
    WHERE codebase_adapter_id IS NOT NULL;

-- Index for finding sessions with codebase adapters by tenant
CREATE INDEX IF NOT EXISTS idx_sessions_codebase_tenant
    ON chat_sessions(tenant_id, codebase_adapter_id)
    WHERE codebase_adapter_id IS NOT NULL;
