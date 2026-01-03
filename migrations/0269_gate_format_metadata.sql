-- Migration 0269: Gate Format Metadata for Deterministic Q15 Quantization
-- Purpose: Store gate quantization format alongside routing decisions to prevent silent drift
-- Related: Issue #2 - Q15 denominator centralization
-- Author: JKCA
-- Date: 2025-01-02

-- Add gate_format column to routing_decisions table.
-- This stores the GateQuantFormat metadata (q_format + denom) as JSON
-- to ensure gates are always decoded with the same parameters they were encoded with.
--
-- Schema:
-- {
--   "q_format": "Q15",
--   "denom": 32767.0
-- }
--
-- Critical Invariant: The denom MUST be 32767.0 for Q15 format.
-- Any deviation indicates a determinism violation.

ALTER TABLE routing_decisions ADD COLUMN gate_format TEXT;

-- Add index for detecting format mismatches (rare but critical)
CREATE INDEX IF NOT EXISTS idx_routing_decisions_gate_format
    ON routing_decisions(gate_format) WHERE gate_format IS NOT NULL;

-- View for detecting non-standard gate formats (audit/repair)
CREATE VIEW IF NOT EXISTS routing_decisions_non_standard_format AS
SELECT
    id,
    tenant_id,
    timestamp,
    request_id,
    gate_format,
    json_extract(gate_format, '$.denom') AS denom
FROM routing_decisions
WHERE gate_format IS NOT NULL
  AND (
    json_extract(gate_format, '$.q_format') != 'Q15'
    OR json_extract(gate_format, '$.denom') != 32767.0
  )
ORDER BY timestamp DESC;
