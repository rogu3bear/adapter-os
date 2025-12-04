-- Migration: PRD-ART-01 Require Signed Adapters Policy
-- Purpose: Add policy knob to enforce Ed25519 signatures on imported .aos adapter files
-- Citation: PRD-ART-01 requirement for policy-controlled adapter signature enforcement

-- Add require_signed_adapters column to tenant_execution_policies
-- Default 0 (false) = unsigned adapters allowed (permissive default)
-- When 1 (true) = imports require valid Ed25519 signatures
ALTER TABLE tenant_execution_policies ADD COLUMN require_signed_adapters INTEGER DEFAULT 0;

-- Update existing policies to permissive default (allow unsigned)
UPDATE tenant_execution_policies SET require_signed_adapters = 0 WHERE require_signed_adapters IS NULL;
