-- Add backend-specific fields to models table for Metal and MLX backends
-- Purpose: Support multiple model loading backends with adapter paths, backend specification, quantization, and error tracking
-- Evidence: Based on Metal and MLX FFI integration requirements for production model loading
-- Merged from crate version: Added last_error, CHECK constraint, and improved indexing

-- Add backend-specific model loading fields
ALTER TABLE models ADD COLUMN adapter_path TEXT;
ALTER TABLE models ADD COLUMN backend TEXT NOT NULL DEFAULT 'metal' CHECK(backend IN ('metal', 'mlx-ffi'));
ALTER TABLE models ADD COLUMN quantization TEXT;
ALTER TABLE models ADD COLUMN last_error TEXT;

-- Update status values to support loading states
-- Note: SQLite doesn't enforce CHECK constraints on existing data, so we update the status values
UPDATE models SET status = 'unloaded' WHERE status = 'available';
UPDATE models SET status = 'unloaded' WHERE status IS NULL;

-- Update existing records to have default backend
UPDATE models SET backend = 'metal' WHERE backend IS NULL;

-- Add composite index for tenant-model lookups (efficient for loading operations)
CREATE INDEX IF NOT EXISTS idx_models_tenant_model ON models(tenant_id, id);

-- Add index for backend queries
CREATE INDEX IF NOT EXISTS idx_models_backend ON models(backend);
