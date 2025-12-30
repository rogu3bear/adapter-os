-- Dataset Collection Sessions Table
-- Migration: 0245
-- Renumbered: Was 0244, renumbered to 0245 when 0242_dataset_repo_slug.sql was inserted
-- Purpose: Track sessions that group multiple dataset operations for atomic workflows
--
-- This migration creates a table to track collection sessions that can span
-- multiple dataset creation/update operations. Sessions provide:
-- 1. Atomic rollback capability for multi-step operations
-- 2. Workflow correlation across datasets and adapters
-- 3. Human-readable names and tags for categorization
--
-- Evidence: Feature requirement for Set 6 - dataset collection tracking
-- Pattern: Session aggregation with tagging and metadata support

CREATE TABLE IF NOT EXISTS dataset_collection_sessions (
    id TEXT PRIMARY KEY,
    -- Human-readable session name (e.g., "nightly-build", "pr-123-review")
    name TEXT NOT NULL,
    -- Comma-separated tags for categorization (e.g., "ci,production,nightly")
    tags TEXT,
    -- Session description for documentation
    description TEXT,
    -- Session state: 'active', 'completed', 'rolled_back', 'failed'
    status TEXT NOT NULL DEFAULT 'active',
    -- Parent session for nested/hierarchical workflows
    parent_session_id TEXT REFERENCES dataset_collection_sessions(id) ON DELETE SET NULL,
    -- Correlation ID from external systems (e.g., CI job ID, PR number)
    external_correlation_id TEXT,
    -- Number of datasets created/modified in this session
    dataset_count INTEGER NOT NULL DEFAULT 0,
    -- Number of adapters trained/registered in this session
    adapter_count INTEGER NOT NULL DEFAULT 0,
    -- Timestamp when session started
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- Timestamp when session ended (completed/failed/rolled_back)
    ended_at TEXT,
    -- Duration in seconds (computed on completion)
    duration_seconds REAL,
    -- User/service that initiated the session
    initiated_by TEXT,
    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    -- Error message if session failed
    error_message TEXT,
    -- Stack trace or detailed error info
    error_details TEXT,
    -- Additional metadata as JSON
    metadata_json TEXT,
    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for active sessions by tenant
CREATE INDEX IF NOT EXISTS idx_dcs_tenant_status ON dataset_collection_sessions(tenant_id, status);

-- Index for session lookup by name
CREATE INDEX IF NOT EXISTS idx_dcs_name ON dataset_collection_sessions(name);

-- Index for finding sessions by tags (partial text match)
CREATE INDEX IF NOT EXISTS idx_dcs_tags ON dataset_collection_sessions(tags) WHERE tags IS NOT NULL;

-- Index for external correlation (e.g., find session by CI job ID)
CREATE INDEX IF NOT EXISTS idx_dcs_external_id ON dataset_collection_sessions(external_correlation_id) WHERE external_correlation_id IS NOT NULL;

-- Index for time-based queries
CREATE INDEX IF NOT EXISTS idx_dcs_started_at ON dataset_collection_sessions(started_at DESC);

-- Index for parent session hierarchy
CREATE INDEX IF NOT EXISTS idx_dcs_parent ON dataset_collection_sessions(parent_session_id) WHERE parent_session_id IS NOT NULL;

-- Junction table linking datasets to their collection sessions
CREATE TABLE IF NOT EXISTS dataset_session_membership (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES dataset_collection_sessions(id) ON DELETE CASCADE,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    -- Type of operation: 'created', 'updated', 'deleted'
    operation_type TEXT NOT NULL DEFAULT 'created',
    -- Order within the session (for sequential operations)
    ordinal INTEGER NOT NULL DEFAULT 0,
    -- Timestamp when this dataset was added to the session
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- UNIQUE constraint to prevent duplicate memberships
    UNIQUE(session_id, dataset_id)
);

-- Index for finding all datasets in a session
CREATE INDEX IF NOT EXISTS idx_dsm_session ON dataset_session_membership(session_id);

-- Index for finding all sessions that include a dataset
CREATE INDEX IF NOT EXISTS idx_dsm_dataset ON dataset_session_membership(dataset_id);

-- Junction table linking adapters to their collection sessions
CREATE TABLE IF NOT EXISTS adapter_session_membership (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES dataset_collection_sessions(id) ON DELETE CASCADE,
    adapter_id TEXT NOT NULL,
    -- Type of operation: 'trained', 'registered', 'promoted', 'deprecated'
    operation_type TEXT NOT NULL DEFAULT 'trained',
    -- Order within the session
    ordinal INTEGER NOT NULL DEFAULT 0,
    -- Timestamp when this adapter was added to the session
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- UNIQUE constraint to prevent duplicate memberships
    UNIQUE(session_id, adapter_id)
);

-- Index for finding all adapters in a session
CREATE INDEX IF NOT EXISTS idx_asm_session ON adapter_session_membership(session_id);

-- Index for finding all sessions that include an adapter
CREATE INDEX IF NOT EXISTS idx_asm_adapter ON adapter_session_membership(adapter_id);
