-- Migration: Base Models Registry Enhancement
-- Purpose: Add fields for model import tracking, format detection, and capabilities
-- PRD-MODEL-01: Make Base Models page reflect real models on disk
-- Citation: Based on existing model schema in 0053_add_model_metadata.sql and 0055_add_model_backend_fields.sql

-- Add registry fields to models table
ALTER TABLE models ADD COLUMN size_bytes INTEGER;
ALTER TABLE models ADD COLUMN format TEXT; -- 'mlx', 'safetensors', 'pytorch', 'gguf'
ALTER TABLE models ADD COLUMN capabilities TEXT; -- JSON array of strings: ["chat", "completion", "embeddings"]
ALTER TABLE models ADD COLUMN import_status TEXT DEFAULT 'available' CHECK(import_status IN ('importing', 'available', 'failed'));
ALTER TABLE models ADD COLUMN import_error TEXT;
ALTER TABLE models ADD COLUMN imported_at TEXT;
ALTER TABLE models ADD COLUMN imported_by TEXT;

-- Create index for format queries
CREATE INDEX IF NOT EXISTS idx_models_format ON models(format);

-- Create index for import status queries
CREATE INDEX IF NOT EXISTS idx_models_import_status ON models(import_status);

-- Update existing models to have available status
UPDATE models SET import_status = 'available' WHERE import_status IS NULL;
UPDATE models SET imported_at = created_at WHERE imported_at IS NULL;
