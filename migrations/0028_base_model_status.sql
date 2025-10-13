-- Migration: Base Model Status Tracking
-- Purpose: Track persistent base model loading state for UX visibility
-- Evidence: Based on existing adapter lifecycle patterns in adapteros-lora-lifecycle

-- Base model status tracking table
CREATE TABLE base_model_status (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('loading', 'loaded', 'unloading', 'unloaded', 'error')),
    loaded_at TEXT,
    unloaded_at TEXT,
    error_message TEXT,
    memory_usage_mb INTEGER,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE
);

-- Index for efficient tenant-based queries
CREATE INDEX idx_base_model_status_tenant_id ON base_model_status(tenant_id);

-- Index for status queries
CREATE INDEX idx_base_model_status_status ON base_model_status(status);

-- Index for model_id lookups
CREATE INDEX idx_base_model_status_model_id ON base_model_status(model_id);

-- Trigger to update updated_at timestamp
CREATE TRIGGER update_base_model_status_updated_at
    AFTER UPDATE ON base_model_status
    FOR EACH ROW
    BEGIN
        UPDATE base_model_status 
        SET updated_at = datetime('now') 
        WHERE id = NEW.id;
    END;

-- Insert initial status for existing tenants (if any models exist)
INSERT INTO base_model_status (tenant_id, model_id, status)
SELECT 
    'default' as tenant_id,
    id as model_id,
    'unloaded' as status
FROM models 
WHERE id NOT IN (SELECT model_id FROM base_model_status);
