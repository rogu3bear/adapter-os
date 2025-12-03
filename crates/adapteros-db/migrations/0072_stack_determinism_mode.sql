-- Migration: Add determinism_mode to adapter_stacks
-- Purpose: Allow per-stack determinism configuration (strict, besteffort, relaxed)

ALTER TABLE adapter_stacks ADD COLUMN determinism_mode TEXT;

-- Index for efficient filtering by determinism mode
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_determinism_mode
    ON adapter_stacks(determinism_mode)
    WHERE determinism_mode IS NOT NULL;
