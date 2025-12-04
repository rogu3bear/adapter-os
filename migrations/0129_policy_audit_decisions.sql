-- Policy audit decisions with Merkle chain
-- Logs every policy decision (allow/deny) for audit trail

CREATE TABLE IF NOT EXISTS policy_audit_decisions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    policy_pack_id TEXT NOT NULL,
    hook TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT,
    request_id TEXT,
    user_id TEXT,
    resource_type TEXT,
    resource_id TEXT,
    metadata_json TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    entry_hash TEXT NOT NULL,
    previous_hash TEXT,
    chain_sequence INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pad_tenant ON policy_audit_decisions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_pad_policy ON policy_audit_decisions(policy_pack_id);
CREATE INDEX IF NOT EXISTS idx_pad_hook ON policy_audit_decisions(hook);
CREATE INDEX IF NOT EXISTS idx_pad_decision ON policy_audit_decisions(decision);
CREATE INDEX IF NOT EXISTS idx_pad_timestamp ON policy_audit_decisions(timestamp);
CREATE INDEX IF NOT EXISTS idx_pad_chain ON policy_audit_decisions(chain_sequence);
CREATE INDEX IF NOT EXISTS idx_pad_request ON policy_audit_decisions(request_id);
