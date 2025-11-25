-- ============================================================================
-- Add Primary Dataset Link to Adapters (PRD-DATA-01)
-- ============================================================================
-- File: migrations/0086_adapters_primary_dataset.sql
-- Purpose: Link adapters to their primary training dataset (cp-evidence-004)
-- Status: New migration for PRD-DATA-01 Dataset Lab & Evidence Explorer
-- Dependencies: adapters (0001), training_datasets (0041)
-- Notes: Enables policy enforcement for T1 adapters requiring dataset provenance
-- ============================================================================

-- Add primary_dataset_id to track which dataset was used to train the adapter
ALTER TABLE adapters ADD COLUMN primary_dataset_id TEXT REFERENCES training_datasets(id) ON DELETE SET NULL;

-- Add eval_dataset_id for production adapters
ALTER TABLE adapters ADD COLUMN eval_dataset_id TEXT REFERENCES training_datasets(id) ON DELETE SET NULL;

-- Create index for dataset lookups
CREATE INDEX idx_adapters_primary_dataset ON adapters(primary_dataset_id);
CREATE INDEX idx_adapters_eval_dataset ON adapters(eval_dataset_id);
