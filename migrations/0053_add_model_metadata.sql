-- Add model_type column to models table
ALTER TABLE models ADD COLUMN model_type TEXT DEFAULT 'base_model';
ALTER TABLE models ADD COLUMN model_path TEXT;
ALTER TABLE models ADD COLUMN config TEXT;
ALTER TABLE models ADD COLUMN status TEXT DEFAULT 'available';
ALTER TABLE models ADD COLUMN tenant_id TEXT DEFAULT 'default';
ALTER TABLE models ADD COLUMN updated_at TEXT DEFAULT (datetime('now'));

-- Add index for tenant queries
CREATE INDEX IF NOT EXISTS idx_models_tenant_id ON models(tenant_id);
