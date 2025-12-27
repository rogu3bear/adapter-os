-- 0230_clusters_topology.sql
-- Adds semantic topology tables for adapter clusters and transition probabilities.

-- Clusters definition table
CREATE TABLE IF NOT EXISTS clusters (
    id TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    default_adapter_id TEXT,
    version TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- Lightweight adapter metadata for topology graph
CREATE TABLE IF NOT EXISTS topology_adapters (
    adapter_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL
);

-- Join table linking adapters to clusters
CREATE TABLE IF NOT EXISTS adapter_clusters (
    adapter_id TEXT NOT NULL,
    cluster_id TEXT NOT NULL,
    PRIMARY KEY (adapter_id, cluster_id),
    FOREIGN KEY(cluster_id) REFERENCES clusters(id)
);

-- Per-adapter transition probabilities (cluster -> next cluster)
CREATE TABLE IF NOT EXISTS adapter_cluster_transitions (
    adapter_id TEXT NOT NULL,
    to_cluster_id TEXT NOT NULL,
    probability REAL NOT NULL,
    PRIMARY KEY (adapter_id, to_cluster_id),
    FOREIGN KEY(to_cluster_id) REFERENCES clusters(id)
);

-- Deterministic ordering helpers
CREATE INDEX IF NOT EXISTS idx_adapter_clusters_cluster_id
ON adapter_clusters(cluster_id, adapter_id);

CREATE INDEX IF NOT EXISTS idx_adapter_cluster_transitions_cluster
ON adapter_cluster_transitions(to_cluster_id, adapter_id);
