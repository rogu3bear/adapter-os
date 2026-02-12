-- Provenance certificate storage
--
-- Each certificate captures the full lineage of an adapter version:
-- training data, checkpoint hashes, promotion history, policy packs,
-- and egress attestations. Signed and content-hashed for tamper detection.

CREATE TABLE IF NOT EXISTS provenance_certificates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    certificate_id TEXT NOT NULL UNIQUE,
    adapter_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,

    -- Training provenance
    training_data_hash TEXT,
    training_config_hash TEXT,
    training_job_id TEXT,
    training_final_loss REAL,
    training_epochs INTEGER,

    -- Checkpoint provenance
    checkpoint_hash TEXT,
    checkpoint_signature TEXT,
    checkpoint_signer_key TEXT,

    -- Promotion provenance
    promotion_review_id TEXT,
    promoted_by TEXT,
    promoted_at TEXT,
    promoted_from_state TEXT,
    promoted_to_state TEXT,

    -- Serving provenance
    policy_pack_hash TEXT,
    policy_pack_id TEXT,
    base_model_id TEXT,

    -- Egress attestation
    egress_blocked INTEGER,           -- 0 or 1
    egress_rules_fingerprint TEXT,

    -- Certificate metadata
    generated_at TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    signature TEXT NOT NULL,
    signer_public_key TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,

    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_provenance_adapter ON provenance_certificates(adapter_id);
CREATE INDEX idx_provenance_version ON provenance_certificates(version_id);
CREATE INDEX idx_provenance_tenant ON provenance_certificates(tenant_id);
CREATE INDEX idx_provenance_generated ON provenance_certificates(generated_at);
