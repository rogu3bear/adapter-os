-- Migration 0143: Add multi-tenant mode flag to tenants
-- Purpose: Allow replay and storage APIs to record tenant isolation mode
-- Created: 2025-12-03

ALTER TABLE tenants
ADD COLUMN multi_tenant_mode TEXT NOT NULL DEFAULT 'disabled';

UPDATE tenants
SET multi_tenant_mode = 'disabled'
WHERE multi_tenant_mode IS NULL;
