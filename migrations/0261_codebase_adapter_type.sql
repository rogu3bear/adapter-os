-- Migration 0261: Add codebase adapter type and stream binding fields
--
-- Purpose: Distinguish codebase adapters as a special class with:
-- - Explicit adapter type (standard, codebase, core)
-- - Required base adapter linkage for codebase adapters
-- - Exclusive stream/session binding
-- - Auto-versioning threshold support
-- - CoreML deployment verification hash
--
-- Evidence: Codebase Adapters PRD - stream-scoped, versioned, deterministic

-- =============================================================================
-- Adapter Type Classification
-- =============================================================================

-- adapter_type: Classify adapters into standard (portable), codebase (stream-scoped), or core (baseline)
-- - standard: Portable adapters like dental-data.aos, can be swapped freely
-- - codebase: Stream-scoped adapters tied to repo state + conversation context
-- - core: Baseline adapters like adapteros.aos, serve as delta base for codebase adapters
ALTER TABLE adapters ADD COLUMN adapter_type TEXT DEFAULT 'standard'
    CHECK (adapter_type IN ('standard', 'codebase', 'core'));

-- =============================================================================
-- Base Adapter Lineage (Distinct from Version Lineage)
-- =============================================================================

-- base_adapter_id: Required for codebase adapters - the core adapter they extend
-- This is different from parent_id which tracks version lineage (v1 -> v2 -> v3)
-- base_adapter_id tracks the delta base (core adapter that codebase builds upon)
-- Note: Using TEXT without FK constraint for SQLite compatibility (adapter_id is not PK)
ALTER TABLE adapters ADD COLUMN base_adapter_id TEXT;

-- =============================================================================
-- Stream Session Binding
-- =============================================================================

-- stream_session_id: Exclusive binding to a chat session
-- Only one codebase adapter can be active per session at a time
ALTER TABLE adapters ADD COLUMN stream_session_id TEXT
    REFERENCES chat_sessions(id) ON DELETE SET NULL;

-- =============================================================================
-- Auto-Versioning Configuration
-- =============================================================================

-- versioning_threshold: Number of activations before auto-versioning triggers
-- When activation_count >= versioning_threshold, system creates new version
-- Default: 100 activations
ALTER TABLE adapters ADD COLUMN versioning_threshold INTEGER DEFAULT 100;

-- =============================================================================
-- CoreML Deployment Verification
-- =============================================================================

-- coreml_package_hash: BLAKE3 hash of fused CoreML package
-- Used to verify deployment matches expected fusion state
-- Only set for adapters that have been fused into CoreML packages
ALTER TABLE adapters ADD COLUMN coreml_package_hash TEXT;

-- =============================================================================
-- Indexes for Codebase Adapter Queries
-- =============================================================================

-- Unique constraint: only one active codebase adapter per session
-- This enforces the "one codebase adapter per stream" invariant
CREATE UNIQUE INDEX IF NOT EXISTS idx_adapters_codebase_session_unique
    ON adapters(stream_session_id)
    WHERE adapter_type = 'codebase'
      AND stream_session_id IS NOT NULL
      AND active = 1;

-- Index for base adapter lineage traversal
CREATE INDEX IF NOT EXISTS idx_adapters_base_adapter_id
    ON adapters(base_adapter_id)
    WHERE base_adapter_id IS NOT NULL;

-- Index for adapter type filtering
CREATE INDEX IF NOT EXISTS idx_adapters_type_tenant
    ON adapters(tenant_id, adapter_type)
    WHERE adapter_type IS NOT NULL;

-- Index for codebase adapters nearing versioning threshold
CREATE INDEX IF NOT EXISTS idx_adapters_versioning_threshold
    ON adapters(adapter_type, activation_count, versioning_threshold)
    WHERE adapter_type = 'codebase';

-- Index for CoreML deployment verification lookups
CREATE INDEX IF NOT EXISTS idx_adapters_coreml_hash
    ON adapters(coreml_package_hash)
    WHERE coreml_package_hash IS NOT NULL;

-- =============================================================================
-- Backfill: Set existing adapters as 'standard' type (already done by DEFAULT)
-- =============================================================================

-- No explicit backfill needed - DEFAULT 'standard' handles existing rows
-- Verify with: SELECT COUNT(*) FROM adapters WHERE adapter_type IS NULL;
