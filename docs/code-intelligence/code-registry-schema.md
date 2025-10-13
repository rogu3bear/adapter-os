# Code Registry Schema Extensions

## Overview

The registry database is extended to support code intelligence metadata while maintaining backward compatibility. New tables are added for repositories, code graphs, and framework mappings.

## New Tables

### repositories

Tracks registered repositories and their metadata.

```sql
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,                    -- e.g., "acme/payments"
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    path TEXT NOT NULL,                     -- Local filesystem path
    languages_json TEXT NOT NULL,           -- ["Python", "TypeScript"]
    frameworks_json TEXT,                   -- [{"name":"django","version":"4.2"}]
    default_branch TEXT DEFAULT 'main',
    latest_scan_commit TEXT,
    latest_scan_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, id)
);

CREATE INDEX idx_repositories_tenant ON repositories(tenant_id);
CREATE INDEX idx_repositories_path ON repositories(path);
```

### code_graphs

Stores CodeGraph metadata and CAS pointers.

```sql
CREATE TABLE IF NOT EXISTS code_graphs (
    id TEXT PRIMARY KEY,                    -- Graph ID: b3 hash
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,           -- Content hash
    file_count INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL,
    test_count INTEGER NOT NULL,
    languages_json TEXT NOT NULL,           -- ["Python", "Rust"]
    frameworks_json TEXT,                   -- [{"name":"django","version":"4.2"}]
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, commit_sha)
);

CREATE INDEX idx_code_graphs_repo ON code_graphs(repo_id);
CREATE INDEX idx_code_graphs_commit ON code_graphs(commit_sha);
CREATE INDEX idx_code_graphs_hash ON code_graphs(hash_b3);
```

### symbol_indices

Pointers to symbol index artifacts.

```sql
CREATE TABLE IF NOT EXISTS symbol_indices (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    index_type TEXT NOT NULL CHECK(index_type IN ('sqlite_fts5', 'tantivy')),
    hash_b3 TEXT UNIQUE NOT NULL,           -- Index artifact hash
    symbol_count INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id, index_type)
);

CREATE INDEX idx_symbol_indices_graph ON symbol_indices(code_graph_id);
```

### vector_indices

Pointers to vector index artifacts.

```sql
CREATE TABLE IF NOT EXISTS vector_indices (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    embedding_model TEXT NOT NULL,          -- e.g., "all-MiniLM-L6-v2"
    embedding_dim INTEGER NOT NULL,
    chunk_count INTEGER NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id, embedding_model)
);

CREATE INDEX idx_vector_indices_graph ON vector_indices(code_graph_id);
```

### test_maps

Precomputed test impact maps.

```sql
CREATE TABLE IF NOT EXISTS test_maps (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    hash_b3 TEXT UNIQUE NOT NULL,
    test_count INTEGER NOT NULL,
    file_coverage_json TEXT NOT NULL,       -- {"file_id": ["test1", "test2"]}
    symbol_coverage_json TEXT NOT NULL,     -- {"symbol_id": ["test1"]}
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id)
);

CREATE INDEX idx_test_maps_graph ON test_maps(code_graph_id);
```

### commit_delta_packs

Ephemeral commit context (CDPs).

```sql
CREATE TABLE IF NOT EXISTS commit_delta_packs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    parent_sha TEXT,
    diff_summary_json TEXT NOT NULL,        -- Changed files, added/removed lines
    changed_symbols_json TEXT NOT NULL,     -- ["symbol_id1", "symbol_id2"]
    test_results_json TEXT,                 -- Failing tests and logs
    lint_results_json TEXT,                 -- Linter/type checker output
    hash_b3 TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,               -- TTL for cleanup
    UNIQUE(repo_id, commit_sha)
);

CREATE INDEX idx_cdp_repo ON commit_delta_packs(repo_id);
CREATE INDEX idx_cdp_commit ON commit_delta_packs(commit_sha);
CREATE INDEX idx_cdp_expires ON commit_delta_packs(expires_at);
```

### ephemeral_sessions

Tracks ephemeral adapter lifecycle.

```sql
CREATE TABLE IF NOT EXISTS ephemeral_sessions (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES adapters(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    cdp_id TEXT REFERENCES commit_delta_packs(id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK(mode IN ('zero_train', 'micro_lora')),
    ttl_seconds INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    evicted_at TEXT,                        -- Null until evicted
    UNIQUE(adapter_id)
);

CREATE INDEX idx_ephemeral_adapter ON ephemeral_sessions(adapter_id);
CREATE INDEX idx_ephemeral_repo_commit ON ephemeral_sessions(repo_id, commit_sha);
CREATE INDEX idx_ephemeral_expires ON ephemeral_sessions(expires_at);
```

## Extended Existing Tables

### adapters (extended)

Add code-specific metadata columns:

```sql
-- New columns added to existing adapters table
ALTER TABLE adapters ADD COLUMN category TEXT CHECK(category IN ('code','framework','codebase','ephemeral'));
ALTER TABLE adapters ADD COLUMN scope TEXT CHECK(scope IN ('global','tenant','repo','commit'));
ALTER TABLE adapters ADD COLUMN framework_id TEXT;
ALTER TABLE adapters ADD COLUMN framework_version TEXT;
ALTER TABLE adapters ADD COLUMN repo_id TEXT REFERENCES repositories(id) ON DELETE CASCADE;
ALTER TABLE adapters ADD COLUMN commit_sha TEXT;
ALTER TABLE adapters ADD COLUMN intent TEXT;
ALTER TABLE adapters ADD COLUMN metadata_json TEXT;

CREATE INDEX idx_adapters_category ON adapters(category);
CREATE INDEX idx_adapters_framework ON adapters(framework_id);
CREATE INDEX idx_adapters_repo ON adapters(repo_id);
CREATE INDEX idx_adapters_commit ON adapters(commit_sha);
```

## Migration SQL

### Migration: 0002_code_intelligence.sql

```sql
-- Add code intelligence tables and extend adapters

BEGIN TRANSACTION;

-- 1. Create repositories table
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    languages_json TEXT NOT NULL,
    frameworks_json TEXT,
    default_branch TEXT DEFAULT 'main',
    latest_scan_commit TEXT,
    latest_scan_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, id)
);

CREATE INDEX idx_repositories_tenant ON repositories(tenant_id);
CREATE INDEX idx_repositories_path ON repositories(path);

-- 2. Create code_graphs table
CREATE TABLE IF NOT EXISTS code_graphs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,
    file_count INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL,
    test_count INTEGER NOT NULL,
    languages_json TEXT NOT NULL,
    frameworks_json TEXT,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, commit_sha)
);

CREATE INDEX idx_code_graphs_repo ON code_graphs(repo_id);
CREATE INDEX idx_code_graphs_commit ON code_graphs(commit_sha);
CREATE INDEX idx_code_graphs_hash ON code_graphs(hash_b3);

-- 3. Create symbol_indices table
CREATE TABLE IF NOT EXISTS symbol_indices (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    index_type TEXT NOT NULL CHECK(index_type IN ('sqlite_fts5', 'tantivy')),
    hash_b3 TEXT UNIQUE NOT NULL,
    symbol_count INTEGER NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id, index_type)
);

CREATE INDEX idx_symbol_indices_graph ON symbol_indices(code_graph_id);

-- 4. Create vector_indices table
CREATE TABLE IF NOT EXISTS vector_indices (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    embedding_model TEXT NOT NULL,
    embedding_dim INTEGER NOT NULL,
    chunk_count INTEGER NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id, embedding_model)
);

CREATE INDEX idx_vector_indices_graph ON vector_indices(code_graph_id);

-- 5. Create test_maps table
CREATE TABLE IF NOT EXISTS test_maps (
    id TEXT PRIMARY KEY,
    code_graph_id TEXT NOT NULL REFERENCES code_graphs(id) ON DELETE CASCADE,
    hash_b3 TEXT UNIQUE NOT NULL,
    test_count INTEGER NOT NULL,
    file_coverage_json TEXT NOT NULL,
    symbol_coverage_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(code_graph_id)
);

CREATE INDEX idx_test_maps_graph ON test_maps(code_graph_id);

-- 6. Create commit_delta_packs table
CREATE TABLE IF NOT EXISTS commit_delta_packs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    parent_sha TEXT,
    diff_summary_json TEXT NOT NULL,
    changed_symbols_json TEXT NOT NULL,
    test_results_json TEXT,
    lint_results_json TEXT,
    hash_b3 TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    UNIQUE(repo_id, commit_sha)
);

CREATE INDEX idx_cdp_repo ON commit_delta_packs(repo_id);
CREATE INDEX idx_cdp_commit ON commit_delta_packs(commit_sha);
CREATE INDEX idx_cdp_expires ON commit_delta_packs(expires_at);

-- 7. Create ephemeral_sessions table
CREATE TABLE IF NOT EXISTS ephemeral_sessions (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES adapters(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    cdp_id TEXT REFERENCES commit_delta_packs(id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK(mode IN ('zero_train', 'micro_lora')),
    ttl_seconds INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    evicted_at TEXT,
    UNIQUE(adapter_id)
);

CREATE INDEX idx_ephemeral_adapter ON ephemeral_sessions(adapter_id);
CREATE INDEX idx_ephemeral_repo_commit ON ephemeral_sessions(repo_id, commit_sha);
CREATE INDEX idx_ephemeral_expires ON ephemeral_sessions(expires_at);

-- 8. Extend adapters table
ALTER TABLE adapters ADD COLUMN category TEXT CHECK(category IN ('code','framework','codebase','ephemeral'));
ALTER TABLE adapters ADD COLUMN scope TEXT CHECK(scope IN ('global','tenant','repo','commit'));
ALTER TABLE adapters ADD COLUMN framework_id TEXT;
ALTER TABLE adapters ADD COLUMN framework_version TEXT;
ALTER TABLE adapters ADD COLUMN repo_id TEXT REFERENCES repositories(id) ON DELETE CASCADE;
ALTER TABLE adapters ADD COLUMN commit_sha TEXT;
ALTER TABLE adapters ADD COLUMN intent TEXT;
ALTER TABLE adapters ADD COLUMN metadata_json TEXT;

CREATE INDEX idx_adapters_category ON adapters(category);
CREATE INDEX idx_adapters_framework ON adapters(framework_id);
CREATE INDEX idx_adapters_repo ON adapters(repo_id);
CREATE INDEX idx_adapters_commit ON adapters(commit_sha);

-- 9. Update schema version
INSERT OR REPLACE INTO schema_version (version, applied_at) VALUES (2, datetime('now'));

COMMIT;
```

## Query Examples

### Register a Repository

```sql
INSERT INTO repositories (id, tenant_id, path, languages_json, frameworks_json)
VALUES (
    'acme/payments',
    'tenant_acme',
    '/repos/acme/payments',
    '["Python", "TypeScript"]',
    '[{"name":"django","version":"4.2"},{"name":"pytest","version":"7.4"}]'
);
```

### Store CodeGraph After Scan

```sql
INSERT INTO code_graphs (id, repo_id, commit_sha, hash_b3, file_count, symbol_count, test_count, languages_json, size_bytes)
VALUES (
    'graph_abc123',
    'acme/payments',
    'abc123def456',
    'b3:fedcba9876543210...',
    142,
    1834,
    287,
    '["Python"]',
    5242880
);
```

### Find CodeGraph for Commit

```sql
SELECT cg.*, si.hash_b3 AS symbol_index_hash, vi.hash_b3 AS vector_index_hash
FROM code_graphs cg
LEFT JOIN symbol_indices si ON si.code_graph_id = cg.id
LEFT JOIN vector_indices vi ON vi.code_graph_id = cg.id
WHERE cg.repo_id = 'acme/payments' AND cg.commit_sha = 'abc123def456';
```

### List Adapters for Repo

```sql
SELECT * FROM adapters
WHERE repo_id = 'acme/payments' AND category = 'codebase'
ORDER BY created_at DESC;
```

### Find Framework Adapters

```sql
SELECT * FROM adapters
WHERE category = 'framework' AND framework_id = 'django'
ORDER BY framework_version DESC;
```

### Check Ephemeral Adapter for Commit

```sql
SELECT es.*, a.name AS adapter_name
FROM ephemeral_sessions es
JOIN adapters a ON a.id = es.adapter_id
WHERE es.repo_id = 'acme/payments' AND es.commit_sha = 'abc123def456'
  AND es.evicted_at IS NULL;
```

### Cleanup Expired CDPs

```sql
DELETE FROM commit_delta_packs
WHERE expires_at < datetime('now');
```

### Evict Expired Ephemeral Adapters

```sql
UPDATE ephemeral_sessions
SET evicted_at = datetime('now')
WHERE expires_at < datetime('now') AND evicted_at IS NULL;
```

## Indexing Strategy

### Symbol Index (SQLite FTS5)

Stored as CAS artifact, loaded on demand:

```sql
-- Within symbol index SQLite file
CREATE VIRTUAL TABLE symbols USING fts5(
    symbol_id UNINDEXED,
    name,
    kind,
    signature,
    docstring,
    file_path,
    content='symbols_data',
    content_rowid='rowid'
);

CREATE TABLE symbols_data (
    rowid INTEGER PRIMARY KEY,
    symbol_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    signature TEXT,
    docstring TEXT,
    file_path TEXT NOT NULL,
    span_json TEXT NOT NULL
);
```

Query example:
```sql
SELECT * FROM symbols
WHERE symbols MATCH 'process_payment OR Payment'
ORDER BY rank
LIMIT 10;
```

## Backward Compatibility

- Existing adapters without `category` default to legacy behavior
- Queries filter `category IS NOT NULL` for code-aware operations
- V1 manifests work unchanged; V4 enables code features

## Storage Considerations

- CodeGraphs: ~1-5 MB per 10K LOC
- Symbol indices: ~500 KB - 2 MB per 10K LOC
- Vector indices: ~5-20 MB per 10K LOC (depending on embedding model)
- Test maps: ~100-500 KB per repo
- CDPs: ~10-100 KB per commit (short-lived)

Per-tenant isolation ensures no cross-contamination.
