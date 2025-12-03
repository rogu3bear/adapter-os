-- Migration: 0128_tenant_token_baseline.sql
-- Purpose: Add per-tenant token revocation baseline for PRD-03
-- Created: 2025-12-02
--
-- This migration adds a token_issued_at_min column to tenants table.
-- Tokens issued before this timestamp are automatically invalidated.
-- This enables bulk revocation of all tokens for a tenant in case of
-- security incidents or tenant migration.

-- Add revocation baseline timestamp column
ALTER TABLE tenants ADD COLUMN token_issued_at_min TEXT;

-- Create index for efficient lookup during auth middleware check
CREATE INDEX IF NOT EXISTS idx_tenants_token_baseline ON tenants(id, token_issued_at_min);
