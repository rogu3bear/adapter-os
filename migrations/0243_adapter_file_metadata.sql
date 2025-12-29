-- ============================================================================
-- ADAPTER FILE METADATA EXTENSION
-- ============================================================================
-- File: migrations/0243_adapter_file_metadata.sql
-- Purpose: Add file size and modification timestamp columns to aos_adapter_metadata
-- for comprehensive adapter file tracking (Set 22 Point 3)
-- ============================================================================

-- Add file size in bytes for disk usage tracking and cache management
ALTER TABLE aos_adapter_metadata ADD COLUMN file_size_bytes INTEGER;

-- Add file modification timestamp for staleness detection
ALTER TABLE aos_adapter_metadata ADD COLUMN file_modified_at TEXT;

-- Add segment count from AOS file for validation
ALTER TABLE aos_adapter_metadata ADD COLUMN segment_count INTEGER;

-- Add manifest schema version for compatibility tracking
ALTER TABLE aos_adapter_metadata ADD COLUMN manifest_schema_version TEXT;

-- Add base model identifier for model compatibility validation
ALTER TABLE aos_adapter_metadata ADD COLUMN base_model TEXT;

-- Add category for filtering
ALTER TABLE aos_adapter_metadata ADD COLUMN category TEXT;

-- Add tier for lifecycle management
ALTER TABLE aos_adapter_metadata ADD COLUMN tier TEXT;

-- Create index on file_size_bytes for disk usage queries
CREATE INDEX IF NOT EXISTS idx_aos_adapter_metadata_file_size ON aos_adapter_metadata(file_size_bytes);

-- Create index on file_modified_at for staleness queries
CREATE INDEX IF NOT EXISTS idx_aos_adapter_metadata_modified_at ON aos_adapter_metadata(file_modified_at);
