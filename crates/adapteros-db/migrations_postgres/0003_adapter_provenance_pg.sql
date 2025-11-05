-- Adapter provenance tracking for cryptographic signer and registrar information
-- Tier 6: Adapters & Clusters Feel Alive

-- Table for storing cryptographic provenance of adapters
CREATE TABLE IF NOT EXISTS adapter_provenance (
    adapter_id TEXT PRIMARY KEY,
    signer_key TEXT NOT NULL,           -- Ed25519 public key that signed the bundle
    registered_by TEXT,                 -- Human registrar (e.g., "ops@shop.example")
    registered_uid INTEGER,             -- Unix UID of registrar
    registered_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    bundle_b3 TEXT NOT NULL,            -- BLAKE3 hash of the adapter bundle
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_adapter_provenance_signer ON adapter_provenance(signer_key);
CREATE INDEX IF NOT EXISTS idx_adapter_provenance_registered_at ON adapter_provenance(registered_at);

-- Table for tracking cross-node replication sessions
CREATE TABLE IF NOT EXISTS replication_journal (
    session_id TEXT PRIMARY KEY,
    from_node TEXT,                     -- Source node ID (NULL for air-gap export)
    to_node TEXT,                       -- Target node ID (NULL for air-gap export)
    bytes BIGINT NOT NULL DEFAULT 0,    -- Total bytes transferred
    artifacts INTEGER NOT NULL DEFAULT 0, -- Number of artifacts replicated
    started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,                  -- NULL if in progress
    result TEXT,                        -- 'success', 'failed', 'partial', etc.
    error_message TEXT,                 -- Error details if result != 'success'
    manifest_b3 TEXT,                   -- BLAKE3 hash of replication manifest
    signature TEXT,                     -- Ed25519 signature of manifest
    FOREIGN KEY (from_node) REFERENCES nodes(id) ON DELETE SET NULL,
    FOREIGN KEY (to_node) REFERENCES nodes(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_replication_journal_started_at ON replication_journal(started_at);
CREATE INDEX IF NOT EXISTS idx_replication_journal_result ON replication_journal(result);
CREATE INDEX IF NOT EXISTS idx_replication_journal_nodes ON replication_journal(from_node, to_node);

-- Table for tracking individual artifacts in replication sessions
CREATE TABLE IF NOT EXISTS replication_artifacts (
    id SERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    artifact_hash TEXT NOT NULL,        -- BLAKE3 hash of artifact
    size_bytes BIGINT NOT NULL,
    transferred_at TIMESTAMP WITH TIME ZONE,                -- NULL if not yet transferred
    verified BOOLEAN NOT NULL DEFAULT false, -- Hash verification status
    FOREIGN KEY (session_id) REFERENCES replication_journal(session_id) ON DELETE CASCADE,
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_replication_artifacts_session ON replication_artifacts(session_id);
CREATE INDEX IF NOT EXISTS idx_replication_artifacts_adapter ON replication_artifacts(adapter_id);
