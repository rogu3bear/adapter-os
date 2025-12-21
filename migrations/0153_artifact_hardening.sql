-- Migration: PRD-ART-01 Artifact Hardening & Portability
-- Purpose: Add schema version tracking, content hash identity, and provenance storage
--          for portable .aos adapter artifacts
-- Citation: PRD-ART-01 requirements for versioned, hash-identified adapter artifacts

-- Add manifest schema version tracking (semantic versioning string)
-- This tracks the JSON manifest structure version, distinct from binary format version
ALTER TABLE adapters ADD COLUMN manifest_schema_version TEXT DEFAULT '1.0.0';

-- Add content hash for identity (BLAKE3 of manifest + weights)
-- This serves as the canonical identity for the adapter across systems
-- Two systems can agree they have "the same adapter" by comparing this hash
ALTER TABLE adapters ADD COLUMN content_hash_b3 TEXT;

-- Add provenance JSON for embedded training provenance
-- Contains: training_job_id, dataset_id, dataset_hash, training_config, documents, export_timestamp
ALTER TABLE adapters ADD COLUMN provenance_json TEXT;

-- Index for schema version queries (useful for compatibility reporting)
CREATE INDEX IF NOT EXISTS idx_adapters_manifest_schema ON adapters(manifest_schema_version);

-- Unique index for content hash lookups (enables deduplication by identity)
-- Partial index: only where content_hash_b3 is not null (existing adapters may not have it)
CREATE UNIQUE INDEX IF NOT EXISTS idx_adapters_content_hash ON adapters(content_hash_b3)
    WHERE content_hash_b3 IS NOT NULL;

-- Index for provenance lookups by training job
CREATE INDEX IF NOT EXISTS idx_adapters_provenance_job ON adapters(
    json_extract(provenance_json, '$.training_job_id')
) WHERE provenance_json IS NOT NULL;
