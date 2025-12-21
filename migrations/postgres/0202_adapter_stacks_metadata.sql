-- Migration: Add metadata_json to adapter_stacks
-- Purpose: Allow stacks to store configuration metadata like dataset_version_id

ALTER TABLE adapter_stacks ADD COLUMN metadata_json TEXT;
