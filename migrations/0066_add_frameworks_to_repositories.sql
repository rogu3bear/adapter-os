-- Migration 0066: Add frameworks support to repositories
-- Adds frameworks_json column to store detected frameworks for repositories

-- Add frameworks_json column to repositories table
ALTER TABLE repositories ADD COLUMN frameworks_json TEXT;

-- Create index for potential future queries on frameworks
CREATE INDEX IF NOT EXISTS idx_repositories_frameworks_json ON repositories(frameworks_json);

-- Note: SQLite doesn't support DEFAULT values on ALTER TABLE ADD COLUMN
-- The column will be NULL for existing rows, which is handled in the application code
