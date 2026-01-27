-- Embedding benchmark results storage
-- Supports audit trail for embedding quality metrics

CREATE TABLE IF NOT EXISTS embedding_benchmarks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    model_name TEXT NOT NULL,
    model_hash TEXT NOT NULL,
    is_finetuned INTEGER NOT NULL DEFAULT 0,
    corpus_version TEXT NOT NULL,
    num_chunks INTEGER NOT NULL,
    recall_at_10 REAL NOT NULL,
    ndcg_at_10 REAL NOT NULL,
    mrr_at_10 REAL NOT NULL,
    determinism_pass INTEGER NOT NULL DEFAULT 1,
    determinism_runs INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

CREATE INDEX IF NOT EXISTS idx_embedding_benchmarks_tenant
    ON embedding_benchmarks(tenant_id);
CREATE INDEX IF NOT EXISTS idx_embedding_benchmarks_timestamp
    ON embedding_benchmarks(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_embedding_benchmarks_model
    ON embedding_benchmarks(model_name);
