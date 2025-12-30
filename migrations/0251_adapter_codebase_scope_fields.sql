-- Migration 0248: Add codebase adapter registration metadata fields
-- These fields support codebase-specific adapter registration with scope tracking
-- for fine-grained code intelligence adapter routing.

-- codebase_scope: Source repository/codebase reference for codebase adapters
-- Used to scope adapter selection to specific codebases or repository patterns
ALTER TABLE adapters ADD COLUMN codebase_scope TEXT;

-- dataset_version_id: Training dataset version ID for reproducibility
-- Links adapter to the specific dataset version used during training
ALTER TABLE adapters ADD COLUMN dataset_version_id TEXT
    REFERENCES training_dataset_versions(id) ON DELETE SET NULL;

-- registration_timestamp: ISO8601 timestamp when adapter was registered
-- Separate from created_at to track explicit registration time from manifests
ALTER TABLE adapters ADD COLUMN registration_timestamp TEXT;

-- manifest_hash: BLAKE3 hash of the adapter manifest for integrity verification
-- Used to detect manifest tampering and ensure reproducible deployments
ALTER TABLE adapters ADD COLUMN manifest_hash TEXT;

-- Index for efficient codebase-scoped adapter lookups
CREATE INDEX IF NOT EXISTS idx_adapters_codebase_scope
    ON adapters(tenant_id, codebase_scope)
    WHERE codebase_scope IS NOT NULL AND active = 1;

-- Index for dataset version tracking
CREATE INDEX IF NOT EXISTS idx_adapters_dataset_version
    ON adapters(dataset_version_id)
    WHERE dataset_version_id IS NOT NULL;
