-- Migration: 0199
-- Purpose: Unified evidence envelope storage for telemetry, policy, and inference evidence
-- PRD: EvidenceEnvelope - Unified Merkle + signature metadata

-- Evidence envelopes table - unified storage for all evidence types
CREATE TABLE IF NOT EXISTS evidence_envelopes (
    -- Primary key
    id TEXT PRIMARY KEY,

    -- Core envelope fields
    schema_version INTEGER NOT NULL DEFAULT 1,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('telemetry', 'policy', 'inference')),

    -- Chain linking (32-byte BLAKE3 hash as hex)
    previous_root TEXT,
    root TEXT NOT NULL,

    -- Signature metadata (Ed25519)
    signature TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_id TEXT NOT NULL,

    -- Optional attestation reference
    attestation_ref TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signed_at_us BIGINT NOT NULL DEFAULT 0,

    -- Scope-specific payload reference (JSON for flexibility)
    payload_json JSONB NOT NULL,

    -- Chain sequence per tenant+scope for ordering
    chain_sequence BIGINT NOT NULL
);

-- Index for tenant isolation and chain traversal
CREATE INDEX IF NOT EXISTS idx_evidence_envelopes_tenant_scope
    ON evidence_envelopes (tenant_id, scope, chain_sequence);

-- Index for chain verification (previous_root lookup)
CREATE INDEX IF NOT EXISTS idx_evidence_envelopes_previous_root
    ON evidence_envelopes (previous_root)
    WHERE previous_root IS NOT NULL;

-- Index for root lookups
CREATE INDEX IF NOT EXISTS idx_evidence_envelopes_root
    ON evidence_envelopes (root);

-- Unique constraint: one root per tenant+scope sequence
CREATE UNIQUE INDEX IF NOT EXISTS idx_evidence_envelopes_tenant_scope_seq
    ON evidence_envelopes (tenant_id, scope, chain_sequence);

-- Index for key_id lookups (signature verification)
CREATE INDEX IF NOT EXISTS idx_evidence_envelopes_key_id
    ON evidence_envelopes (key_id);
