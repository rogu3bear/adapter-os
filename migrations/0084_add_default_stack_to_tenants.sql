-- Add default_stack_id to tenants table
-- Migration: 0084
-- Created: 2025-01-20
-- Purpose: Enable default adapter stack per tenant for chat/inference

ALTER TABLE tenants ADD COLUMN default_stack_id TEXT;

-- Foreign key constraint to adapter_stacks
-- Note: Using ON DELETE SET NULL so deleting a stack doesn't break tenant references
CREATE INDEX idx_tenants_default_stack ON tenants(default_stack_id);

-- Update existing tenants to have NULL default_stack_id (no default initially)
UPDATE tenants SET default_stack_id = NULL WHERE default_stack_id IS NULL;

