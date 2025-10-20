-- ============================================================================
-- AOS COORDINATION HEADER
-- ============================================================================
-- File: migrations/0042_aos_adapters.sql
-- Phase: 2 - System Integration (Database Schema)
-- Assigned: Intern D (Database Team)
-- Status: Complete - Schema changes implemented
-- Dependencies: Adapter table, SingleFileAdapter format
-- Last Updated: 2024-01-15
-- 
-- COORDINATION NOTES:
-- - This file affects: Database schema, adapter storage, queries
-- - Changes require: Updates to database access layer and CLI commands
-- - Testing needed: Database migration tests and schema validation
-- - CLI Impact: CLI commands use these columns for .aos file tracking
-- - UI Impact: UI displays .aos file information from these columns
-- ============================================================================

-- Add .aos adapter support to existing adapters table
ALTER TABLE adapters ADD COLUMN aos_file_path TEXT; -- COORDINATION: Path to .aos file
ALTER TABLE adapters ADD COLUMN aos_file_hash TEXT; -- COORDINATION: Hash of .aos file for integrity

-- Create index for .aos file lookups
CREATE INDEX IF NOT EXISTS idx_adapters_aos_file_hash ON adapters(aos_file_hash);

-- Add .aos adapter metadata table
CREATE TABLE IF NOT EXISTS aos_adapter_metadata (
    adapter_id TEXT PRIMARY KEY, -- COORDINATION: Links to adapters table
    aos_file_path TEXT NOT NULL, -- COORDINATION: Path to .aos file
    aos_file_hash TEXT NOT NULL, -- COORDINATION: Hash for integrity verification
    extracted_weights_path TEXT, -- COORDINATION: Path to extracted weights
    training_data_count INTEGER, -- COORDINATION: Number of training examples
    lineage_version TEXT, -- COORDINATION: Version tracking for lineage
    signature_valid BOOLEAN DEFAULT FALSE, -- COORDINATION: Cryptographic signature status
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
);

-- Create indices for performance
CREATE INDEX IF NOT EXISTS idx_aos_adapter_metadata_file_hash ON aos_adapter_metadata(aos_file_hash);
CREATE INDEX IF NOT EXISTS idx_aos_adapter_metadata_created_at ON aos_adapter_metadata(created_at DESC);
