-- Migration: 0093_audit_chain_of_custody.sql
-- Purpose: Add remaining chain-of-custody field to audit_logs for tamper-evident audit trail
-- Author: AI Assistant (implementing PRD-GOV-01 requirement)
-- Date: 2025-11-25
-- Note: previous_hash and chain_sequence already added in migration 0090

-- Add entry_hash column to audit_logs (previous_hash and chain_sequence already exist from 0090)
ALTER TABLE audit_logs ADD COLUMN entry_hash TEXT;

-- Create index for hash lookups (chain_sequence index already created in 0090)
CREATE INDEX IF NOT EXISTS idx_audit_entry_hash ON audit_logs(entry_hash);
