-- Rollback for stack versioning (0079)
-- Migration: 0079_stack_versioning_rollback.sql
-- Created: 2025-11-19
-- Purpose: Remove stack versioning columns and indexes

-- Remove the version column from adapter_stacks
-- Note: This will lose version information, backup if needed
ALTER TABLE adapter_stacks DROP COLUMN version;

-- Remove the version index
DROP INDEX IF EXISTS idx_adapter_stacks_version;

