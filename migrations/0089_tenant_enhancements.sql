-- Tenant enhancements
-- Migration: 0089
-- Created: 2025-11-25
-- Purpose: Add status, timestamps, and limits/quotas to tenants table

-- Add status column (active, paused, archived)
ALTER TABLE tenants ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
    CHECK(status IN ('active', 'paused', 'archived'));

-- Add updated_at timestamp for tracking changes
ALTER TABLE tenants ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));

-- Tenant resource limits
ALTER TABLE tenants ADD COLUMN max_adapters INTEGER DEFAULT NULL;
ALTER TABLE tenants ADD COLUMN max_training_jobs INTEGER DEFAULT NULL;
ALTER TABLE tenants ADD COLUMN max_storage_gb REAL DEFAULT NULL;
ALTER TABLE tenants ADD COLUMN rate_limit_rpm INTEGER DEFAULT 1000;

-- Create index for status queries
CREATE INDEX IF NOT EXISTS idx_tenants_status ON tenants(status);

-- Create index for updated_at (for sorting/filtering by last update)
CREATE INDEX IF NOT EXISTS idx_tenants_updated_at ON tenants(updated_at DESC);

-- Update trigger to automatically set updated_at on changes
CREATE TRIGGER IF NOT EXISTS update_tenants_timestamp
AFTER UPDATE ON tenants
FOR EACH ROW
BEGIN
    UPDATE tenants SET updated_at = datetime('now') WHERE id = NEW.id;
END;
