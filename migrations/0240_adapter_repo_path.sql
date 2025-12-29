-- Migration 0243: Add repo_path column to adapters table for scan root persistence
--
-- Previously, repo_path (the canonicalized scan root) was only stored in .aos
-- package metadata. This migration adds it to the adapters table for direct
-- database queries without needing to read the .aos file.
--
-- See: Set 12 Point 1 - Record repo scope and scan root

-- Add repo_path column to adapters table
ALTER TABLE adapters ADD COLUMN repo_path TEXT;

-- Add index for repo_path lookups (useful for finding adapters by source repo location)
CREATE INDEX IF NOT EXISTS idx_adapters_repo_path ON adapters(repo_path) WHERE repo_path IS NOT NULL;

-- Composite index for tenant + repo_path queries
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_repo_path ON adapters(tenant_id, repo_path) WHERE repo_path IS NOT NULL;
