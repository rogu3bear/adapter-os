-- Migration 0139: Add pinned adapters support
-- Per-tenant defaults + per-session overrides
--
-- Pinned adapters are stored as JSON arrays: ["adapter-id-1", "adapter-id-2"]
-- NULL means "no pinned adapters"
-- Inheritance: new sessions inherit from tenant default if not explicitly provided

-- Add tenant default pinned adapters
ALTER TABLE tenants ADD COLUMN default_pinned_adapter_ids TEXT;

-- Add session pinned adapters
ALTER TABLE chat_sessions ADD COLUMN pinned_adapter_ids TEXT;
