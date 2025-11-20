-- Re-add aos_file_path and aos_file_hash columns to adapters table
-- Citation: PRD-02 .aos Upload Integration (Agent 9 - Integration Verifier)
--
-- Background:
-- - Migration 0045 added aos_file_path and aos_file_hash columns
-- - Migration 0059 removed them as "unused" (zero grep matches at that time)
-- - PRD-02 now implements .aos upload functionality and requires these columns
--
-- This migration re-adds the columns to support:
-- 1. Direct .aos file uploads via API
-- 2. Database tracking of .aos file locations
-- 3. Integration with aos_adapter_metadata table (foreign key relationship)

-- Temporarily disable foreign key constraints during schema modification
-- This is necessary because ALTER TABLE ADD COLUMN can cause foreign key
-- mismatch errors in tables that reference adapters (like adapter_version_history)
PRAGMA foreign_keys=OFF;

-- Re-add aos_file_path column (nullable - not all adapters have .aos files)
ALTER TABLE adapters ADD COLUMN aos_file_path TEXT;

-- Re-add aos_file_hash column (nullable - not all adapters have .aos files)
ALTER TABLE adapters ADD COLUMN aos_file_hash TEXT;

-- Create index for efficient lookup by hash
CREATE INDEX IF NOT EXISTS idx_adapters_aos_file_hash ON adapters(aos_file_hash);

-- Re-enable foreign key constraints
PRAGMA foreign_keys=ON;

-- Note: The aos_adapter_metadata table already exists (created in migration 0045)
-- and has foreign key constraint to adapters(id). This migration ensures
-- consistency between the two tables by restoring the columns in adapters.

-- Verification queries:
-- SELECT name, type FROM pragma_table_info('adapters') WHERE name IN ('aos_file_path', 'aos_file_hash');
-- SELECT name FROM pragma_index_list('adapters') WHERE name = 'idx_adapters_aos_file_hash';
