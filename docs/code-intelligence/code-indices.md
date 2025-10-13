# Code Index Specifications

## Overview

Code intelligence uses three types of indices: symbol index (SQLite FTS5), vector index (HNSW), and test map (JSON). All indices are per-repository, content-addressed, and stored in CAS.

---

## Symbol Index

### Purpose
Fast full-text search for symbols (functions, classes, methods) by name, signature, or docstring.

### Technology
SQLite with FTS5 (Full-Text Search) extension.

### Schema

```sql
-- Main data table
CREATE TABLE symbols_data (
    rowid INTEGER PRIMARY KEY,
    symbol_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,           -- Function, Class, Method, etc.
    signature TEXT,
    docstring TEXT,
    file_path TEXT NOT NULL,
    file_id TEXT NOT NULL,
    span_json TEXT NOT NULL,      -- {"start_line":10,"start_col":0,"end_line":20,"end_col":0}
    visibility TEXT,              -- Public, private, internal
    language TEXT NOT NULL
);

-- FTS5 virtual table
CREATE VIRTUAL TABLE symbols USING fts5(
    symbol_id UNINDEXED,
    name,
    kind UNINDEXED,
    signature,
    docstring,
    file_path UNINDEXED,
    content='symbols_data',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Auxiliary indices
CREATE INDEX idx_symbols_name ON symbols_data(name);
CREATE INDEX idx_symbols_kind ON symbols_data(kind);
CREATE INDEX idx_symbols_file ON symbols_data(file_id);
CREATE INDEX idx_symbols_language ON symbols_data(language);
```

### Query Patterns

**1. Exact name match**:
```sql
SELECT * FROM symbols
WHERE symbols MATCH 'name:process_payment'
ORDER BY rank
LIMIT 10;
```

**2. Fuzzy name search**:
```sql
SELECT * FROM symbols
WHERE symbols MATCH 'name:process_payment OR Payment'
ORDER BY rank
LIMIT 10;
```

**3. By kind**:
```sql
SELECT * FROM symbols_data
WHERE kind = 'Function' AND name LIKE 'process%'
LIMIT 10;
```

**4. In specific file**:
```sql
SELECT * FROM symbols_data
WHERE file_path LIKE 'src/payments/%'
ORDER BY name
LIMIT 50;
```

**5. Docstring search**:
```sql
SELECT * FROM symbols
WHERE symbols MATCH 'docstring:"payment timeout"'
ORDER BY rank
LIMIT 10;
```

### Performance

- Index size: ~500 KB - 2 MB per 10K LOC
- Lookup time: <10ms (indexed)
- Full-text search: <50ms for most queries
- Memory footprint: ~10-20 MB loaded

---

## Vector Index

### Purpose
Semantic search for code chunks using embeddings.

### Technology
HNSW (Hierarchical Navigable Small World) for approximate nearest neighbor search.

### Structure

```rust
pub struct VectorIndex {
    pub dimension: usize,              // Embedding dimension (e.g., 384)
    pub count: usize,                  // Number of chunks
    pub hnsw: HnswIndex,               // HNSW graph
    pub metadata: Vec<ChunkMetadata>,  // Per-chunk metadata
}

pub struct ChunkMetadata {
    pub chunk_id: String,
    pub file_id: String,
    pub file_path: String,
    pub symbol_id: Option<String>,
    pub span: Span,
    pub language: Language,
    pub symbol_kind: Option<SymbolKind>,
    pub is_test: bool,
}
```

### Chunking Strategy

**Symbol-aware chunking**:
- Primary: One chunk per symbol (function/class/method)
- Context: Include 5 lines before/after for context
- Max chunk size: 500 tokens (~400 lines)

**File-level chunking** (for files without symbols):
- README, config files, docs
- Fixed window: 50 lines per chunk
- Overlap: 5 lines between chunks

### Embedding Model

**Recommended**: `all-MiniLM-L6-v2`
- Dimension: 384
- Quality: Good balance of speed/accuracy
- Speed: ~1000 chunks/second (CPU), ~10K/second (GPU)

**Alternative**: `code-search-net` (code-specific)
- Dimension: 768
- Quality: Better for code, slower
- Speed: ~400 chunks/second (CPU)

### HNSW Parameters

```rust
HnswConfig {
    m: 16,              // Max connections per layer
    ef_construction: 200, // Quality during build
    ef_search: 50,      // Quality during search
    max_elements: 100000,
    distance: Cosine,
}
```

### Query Flow

```rust
pub fn search(
    &self,
    query_embedding: &[f32],
    k: usize,
    filters: &Filters,
) -> Result<Vec<SearchResult>> {
    // 1. HNSW search
    let candidates = self.hnsw.search(query_embedding, k * 10)?;
    
    // 2. Apply filters
    let filtered: Vec<_> = candidates
        .into_iter()
        .filter(|c| self.apply_filters(c, filters))
        .collect();
    
    // 3. Sort by score and take top-k
    let results: Vec<SearchResult> = filtered
        .into_iter()
        .take(k)
        .map(|c| self.build_result(c))
        .collect();
    
    Ok(results)
}
```

### Filters

```rust
pub struct Filters {
    pub language: Option<Language>,
    pub file_pattern: Option<String>,     // Glob pattern
    pub symbol_kind: Option<SymbolKind>,
    pub is_test: Option<bool>,
    pub min_score: f32,
}
```

### Performance

- Index size: ~5-20 MB per 10K LOC (depends on model)
- Build time: ~30s (GPU), ~120s (CPU) per 10K LOC
- Search time: <100ms for k=5
- Memory footprint: ~50-200 MB loaded

---

## Test Map

### Purpose
Precomputed mapping of files/symbols to tests for impact analysis.

### Format

JSON structure:

```json
{
  "repo_id": "acme/payments",
  "commit_sha": "abc123def456",
  "test_count": 287,
  "file_coverage": {
    "file_abc123": ["test_def456", "test_ghi789"],
    "file_jkl012": ["test_mno345"]
  },
  "symbol_coverage": {
    "sym_abc123": ["test_def456"],
    "sym_pqr678": ["test_def456", "test_ghi789"]
  },
  "test_metadata": {
    "test_def456": {
      "name": "test_process_payment_success",
      "file": "tests/test_processor.py",
      "kind": "unit",
      "runtime_ms": 45
    }
  }
}
```

### Building the Map

```rust
pub fn build_test_map(graph: &CodeGraph) -> Result<TestMap> {
    let mut file_coverage = HashMap::new();
    let mut symbol_coverage = HashMap::new();
    
    for test in graph.tests() {
        // Direct coverage (explicit test_covers edges)
        for symbol_id in graph.test_targets(test.id) {
            symbol_coverage.entry(symbol_id).or_insert_with(Vec::new).push(test.id);
            
            let file_id = graph.file_for_symbol(symbol_id)?;
            file_coverage.entry(file_id).or_insert_with(Vec::new).push(test.id);
        }
        
        // Indirect coverage (imports from test file)
        let test_file = graph.file_for_symbol(test.id)?;
        for import in graph.imports_from(test_file) {
            let target_file = import.to;
            file_coverage.entry(target_file).or_insert_with(Vec::new).push(test.id);
            
            // Add all symbols in imported file
            for symbol in graph.symbols_in_file(target_file) {
                symbol_coverage.entry(symbol.id).or_insert_with(Vec::new).push(test.id);
            }
        }
    }
    
    // Deduplicate
    for tests in file_coverage.values_mut() {
        tests.sort();
        tests.dedup();
    }
    for tests in symbol_coverage.values_mut() {
        tests.sort();
        tests.dedup();
    }
    
    Ok(TestMap { file_coverage, symbol_coverage })
}
```

### Query Patterns

**Find tests for a file**:
```rust
let tests = test_map.file_coverage.get(&file_id).unwrap_or(&vec![]);
```

**Find tests for a symbol**:
```rust
let tests = test_map.symbol_coverage.get(&symbol_id).unwrap_or(&vec![]);
```

**Compute impact for changed files**:
```rust
let mut impacted = HashSet::new();
for file_id in changed_files {
    if let Some(tests) = test_map.file_coverage.get(file_id) {
        impacted.extend(tests);
    }
}
```

### Performance

- Map size: ~100-500 KB per 10K LOC
- Build time: <2s
- Lookup time: <1ms (hash map)
- Memory footprint: ~5-10 MB loaded

---

## Index Lifecycle

### Creation

1. **Scan repository** → CodeGraph
2. **Extract symbols** → Symbol index (SQLite)
3. **Chunk & embed** → Vector index (HNSW)
4. **Analyze tests** → Test map (JSON)
5. **Hash all artifacts** → BLAKE3
6. **Store in CAS** → Content-addressed storage
7. **Register in registry** → Metadata + pointers

### Loading

```rust
pub async fn load_indices(
    repo_id: &str,
    commit: &str,
    registry: &Registry,
    cas: &CasStore,
) -> Result<Indices> {
    // 1. Lookup graph metadata
    let graph_meta = registry.get_code_graph(repo_id, commit).await?;
    
    // 2. Load symbol index
    let symbol_hash = registry.get_symbol_index_hash(&graph_meta.id).await?;
    let symbol_bytes = cas.get(&symbol_hash).await?;
    let symbol_path = temp_file();
    std::fs::write(&symbol_path, symbol_bytes)?;
    let symbol_index = SymbolIndex::open(&symbol_path)?;
    
    // 3. Load vector index
    let vector_hash = registry.get_vector_index_hash(&graph_meta.id).await?;
    let vector_bytes = cas.get(&vector_hash).await?;
    let vector_index = VectorIndex::deserialize(&vector_bytes)?;
    
    // 4. Load test map
    let test_map_hash = registry.get_test_map_hash(&graph_meta.id).await?;
    let test_map_bytes = cas.get(&test_map_hash).await?;
    let test_map: TestMap = serde_json::from_slice(&test_map_bytes)?;
    
    Ok(Indices {
        symbol_index,
        vector_index,
        test_map,
    })
}
```

### Caching

Indices are cached per-tenant to avoid repeated CAS fetches:

```rust
pub struct IndexCache {
    symbol_indices: LruCache<(RepoId, CommitSha), Arc<SymbolIndex>>,
    vector_indices: LruCache<(RepoId, CommitSha), Arc<VectorIndex>>,
    test_maps: LruCache<(RepoId, CommitSha), Arc<TestMap>>,
    max_size: usize,
}
```

### Invalidation

Indices are invalidated when:
- New commit is scanned (creates new indices)
- Repository is re-indexed
- TTL expires (optional, for space reclamation)

---

## Per-Tenant Isolation

All indices are strictly isolated per tenant:

1. **Registry entries** enforce tenant_id foreign keys
2. **CAS storage** uses tenant-scoped directories
3. **Cache** uses (tenant_id, repo_id, commit) as key
4. **API** validates tenant authorization before loading

Cross-tenant access is impossible by design.

---

## Storage Estimates

For a medium-sized repository (50K LOC):

| Artifact        | Size       | Notes                          |
|-----------------|------------|--------------------------------|
| CodeGraph       | 5-10 MB    | Binary serialization           |
| Symbol Index    | 2-5 MB     | SQLite FTS5                    |
| Vector Index    | 25-100 MB  | Depends on embedding model     |
| Test Map        | 500 KB-2MB | JSON                           |
| **Total**       | **33-117 MB** | Per commit                 |

Typical repository with 10 commits tracked: **330 MB - 1.2 GB**

With deduplication (shared base layers), storage can be reduced by ~40%.

---

## Determinism

All indices are deterministic:

- **Symbol index**: Sorted insertion, stable FTS5 tokenization
- **Vector index**: Seeded HNSW construction, stable embedding model
- **Test map**: Sorted keys and values

Given identical repository state and tool versions, indices are byte-identical across builds.
