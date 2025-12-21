-- Adapter Training Job Link
-- Migration: 0101
-- Purpose: Add direct link from adapters to their training jobs
--
-- This enables:
-- - "What job created this adapter?" queries
-- - Reverse lookup from adapter to full training provenance
-- - Complete traceability without searching metadata_json
--
-- Evidence: Previously, the link was only stored in training_job.metadata_json.adapter_id
-- which required JSON searching for lookups. This FK provides direct access.

-- Add training_job_id to adapters for direct provenance lookup
ALTER TABLE adapters ADD COLUMN training_job_id TEXT REFERENCES repository_training_jobs(id) ON DELETE SET NULL;

-- Index for reverse lookups (find adapters by training job)
CREATE INDEX IF NOT EXISTS idx_adapters_training_job ON adapters(training_job_id);
