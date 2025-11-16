-- Remove unused columns from adapters table (migration 0045)
-- Citation: Multi-agent schema audit - Agent B findings
-- These columns were added but never referenced in Rust code
-- Priority: LOW (cleanup only, minimal space savings)

-- ============================================================================
-- Migration 0045: AOS Adapter Fields (PARTIAL USAGE)
-- ============================================================================
-- The aos_file_path and aos_file_hash columns were added but:
-- - Zero grep matches in Rust codebase
-- - Not included in any struct definitions
-- - Not referenced in INSERT or SELECT queries

-- SQLite doesn't support DROP COLUMN directly in older versions,
-- so we use the standard ALTER TABLE ... DROP COLUMN syntax
-- which works in SQLite 3.35.0+ (2021-03-12)

-- Drop indexes on the columns first (migration 0045 created idx_adapters_aos_file_hash)
DROP INDEX IF EXISTS idx_adapters_aos_file_hash;

-- Drop aos_file_path and aos_file_hash columns
-- Note: SQLite 3.35.0+ supports DROP COLUMN
-- Indexes must be dropped first before dropping columns they reference
ALTER TABLE adapters DROP COLUMN aos_file_path;
ALTER TABLE adapters DROP COLUMN aos_file_hash;

-- Note: If you get an error "Cannot drop column", you're on SQLite < 3.35.0
-- In that case, use the table recreation approach:
--
-- -- Alternative for old SQLite:
-- ALTER TABLE adapters RENAME TO adapters_old;
--
-- CREATE TABLE adapters (
--     id TEXT PRIMARY KEY,
--     name TEXT NOT NULL,
--     tier TEXT NOT NULL,
--     hash_b3 TEXT UNIQUE NOT NULL,
--     rank INTEGER NOT NULL,
--     alpha REAL NOT NULL,
--     -- ... all other columns EXCEPT aos_file_path and aos_file_hash
-- );
--
-- INSERT INTO adapters SELECT
--     id, name, tier, hash_b3, rank, alpha, ...
-- FROM adapters_old;
--
-- DROP TABLE adapters_old;

-- Verification: Ensure core adapter fields remain:
-- - id, name, tier, hash_b3, rank, alpha (core fields from 0001)
-- - load_state, last_loaded_at (from 0031)
-- - expires_at (from 0044)
