-- Add decision chain hash and environment identity to diag_runs
-- Evidence: Crypto Audit Receipts Infrastructure
--
-- The decision_chain_hash commits to the Merkle root of all router decisions
-- for a given inference run. This enables:
-- - Deterministic replay verification
-- - Tamper-evident audit trails
-- - Chain-of-custody for routing decisions

ALTER TABLE diag_runs ADD COLUMN decision_chain_hash TEXT;
ALTER TABLE diag_runs ADD COLUMN backend_identity_hash TEXT;
ALTER TABLE diag_runs ADD COLUMN model_identity_hash TEXT;
ALTER TABLE diag_runs ADD COLUMN adapter_stack_ids TEXT;

-- Index for querying by decision chain hash (for replay verification)
CREATE INDEX IF NOT EXISTS idx_diag_runs_decision_chain_hash
    ON diag_runs(decision_chain_hash)
    WHERE decision_chain_hash IS NOT NULL;
