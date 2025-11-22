-- ============================================================================
-- Add AOS File Tracking Columns to Adapters
-- ============================================================================
-- File: migrations/0083_add_aos_file_columns.sql
-- Purpose: Add aos_file_path and aos_file_hash columns referenced in adapter code
-- Status: New migration to fix missing columns from migration 0045
-- Dependencies: adapters table (migration 0001)
-- Notes: Migration 0045 defined these columns but they may not exist in all environments
-- ============================================================================

-- Add aos_file_path column if it doesn't exist
-- This tracks the location of the .aos file for the adapter
ALTER TABLE adapters ADD COLUMN aos_file_path TEXT;

-- Add aos_file_hash column if it doesn't exist
-- This stores the BLAKE3 hash of the .aos file for integrity verification
ALTER TABLE adapters ADD COLUMN aos_file_hash TEXT;

-- Create index for .aos file hash lookups
CREATE INDEX idx_adapters_aos_file_hash
    ON adapters(aos_file_hash);
