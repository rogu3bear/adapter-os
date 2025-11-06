-- Migration: Add Model Loading Fields for MLX Backend
-- Purpose: Support MLX model loading with adapter paths, backend specification, quantization, and error tracking
-- Evidence: Based on MLX FFI integration requirements for production model loading

-- Add MLX-specific model loading fields
ALTER TABLE models ADD COLUMN adapter_path TEXT;
ALTER TABLE models ADD COLUMN backend TEXT NOT NULL DEFAULT 'mlx-ffi' CHECK(backend IN ('mlx-ffi', 'metal'));
ALTER TABLE models ADD COLUMN quantization TEXT;
ALTER TABLE models ADD COLUMN last_error TEXT;

-- Update status values to support loading states
-- Note: SQLite doesn't enforce CHECK constraints on existing data, so we update the status values
UPDATE models SET status = 'unloaded' WHERE status = 'available';
UPDATE models SET status = 'unloaded' WHERE status IS NULL;

-- Add composite index for tenant-model lookups (efficient for loading operations)
CREATE INDEX IF NOT EXISTS idx_models_tenant_model ON models(tenant_id, id);
