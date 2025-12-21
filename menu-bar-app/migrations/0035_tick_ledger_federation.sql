-- Extend Tick Ledger with Federation Metadata
-- 
-- Adds federation-specific columns to tick_ledger table for cross-host
-- replay validation and federation chain linkage.
--
-- Per Determinism Ruleset #2 and Federation integration

-- Add federation metadata columns
ALTER TABLE tick_ledger_entries ADD COLUMN bundle_hash TEXT;
ALTER TABLE tick_ledger_entries ADD COLUMN prev_host_hash TEXT;
ALTER TABLE tick_ledger_entries ADD COLUMN federation_signature TEXT;

-- Create indexes for federation queries
CREATE INDEX IF NOT EXISTS idx_tick_ledger_bundle_hash ON tick_ledger_entries(bundle_hash) 
    WHERE bundle_hash IS NOT NULL;
    
CREATE INDEX IF NOT EXISTS idx_tick_ledger_prev_host_hash ON tick_ledger_entries(prev_host_hash) 
    WHERE prev_host_hash IS NOT NULL;

-- View for federation-linked tick entries
CREATE VIEW IF NOT EXISTS tick_ledger_federation AS
SELECT 
    tl.id,
    tl.tick,
    tl.host_id,
    tl.recorded_at,
    tl.tick_hash,
    tl.prev_tick_hash,
    tl.bundle_hash,
    tl.prev_host_hash,
    tl.federation_signature,
    fbs.signature as federation_bundle_signature,
    fbs.verified as federation_verified
FROM tick_ledger_entries tl
LEFT JOIN federation_bundle_signatures fbs ON tl.bundle_hash = fbs.bundle_hash
WHERE tl.bundle_hash IS NOT NULL
ORDER BY tl.recorded_at DESC;

