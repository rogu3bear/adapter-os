-- Add backend-specific fields to models table for MLX and other backends
ALTER TABLE models ADD COLUMN adapter_path TEXT;
ALTER TABLE models ADD COLUMN backend TEXT DEFAULT 'metal';
ALTER TABLE models ADD COLUMN quantization TEXT;

-- Add index for backend queries
CREATE INDEX IF NOT EXISTS idx_models_backend ON models(backend);

-- Update existing records to have default backend
UPDATE models SET backend = 'metal' WHERE backend IS NULL;
