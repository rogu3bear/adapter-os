-- Migration: Add determinism_mode to tenants
-- Purpose: Allow per-tenant default determinism configuration

-- Add determinism_mode column to tenants table
-- Valid values: 'strict', 'besteffort', 'relaxed'
-- NULL means inherit from global config
ALTER TABLE tenants ADD COLUMN determinism_mode TEXT;

-- Index for efficient filtering by determinism mode
CREATE INDEX IF NOT EXISTS idx_tenants_determinism_mode
    ON tenants(determinism_mode)
    WHERE determinism_mode IS NOT NULL;
