-- Migration 0237: Dataset-to-model compatibility tracking
-- Purpose: Allow datasets to declare which base models they are compatible with
-- This enables strict enforcement at training submission time

CREATE TABLE IF NOT EXISTS dataset_model_compatibility (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    -- Compatibility levels:
    -- 'compatible': Dataset can be used with this model (default, permissive)
    -- 'optimized': Dataset was specifically created/validated for this model
    -- 'required': Dataset MUST only be used with this exact model (strict enforcement)
    compatibility_level TEXT NOT NULL DEFAULT 'compatible',
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE,
    UNIQUE(tenant_id, dataset_id, model_id)
);

-- Index for fast lookups by dataset
CREATE INDEX IF NOT EXISTS idx_dataset_model_compat_dataset
    ON dataset_model_compatibility(tenant_id, dataset_id);

-- Index for fast lookups by model
CREATE INDEX IF NOT EXISTS idx_dataset_model_compat_model
    ON dataset_model_compatibility(tenant_id, model_id);
