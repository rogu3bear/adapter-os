-- Migration 0098: Add base_model_id foreign key to adapters
-- Purpose: Link adapters to their base models for tracking and lifecycle management
-- Created: 2025-11-25
-- Dependencies: adapters (0001), models (0001), base_model_registry (0091)

-- Add base_model_id to track which base model the adapter was trained from
ALTER TABLE adapters ADD COLUMN base_model_id TEXT REFERENCES models(id) ON DELETE SET NULL;

-- Create index for base model lookups
CREATE INDEX IF NOT EXISTS idx_adapters_base_model ON adapters(base_model_id);
