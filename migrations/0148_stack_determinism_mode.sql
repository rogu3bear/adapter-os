-- Migration: Add determinism_mode to adapter_stacks
-- Purpose: Allow per-stack determinism configuration override

-- Add determinism_mode column to adapter_stacks table
-- Valid values: 'strict', 'besteffort', 'relaxed'
-- NULL means inherit from tenant or global config
ALTER TABLE adapter_stacks ADD COLUMN determinism_mode TEXT;

-- Index for efficient filtering by determinism mode
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_determinism_mode
    ON adapter_stacks(determinism_mode)
    WHERE determinism_mode IS NOT NULL;
