# Evidence Retrieval System

## Overview

AdapterOS includes a comprehensive evidence retrieval system that provides evidence-grounded responses by indexing and searching across multiple evidence types: symbols, tests, documentation, and code chunks. This system ensures all AI responses are backed by verifiable code evidence with full provenance tracking.

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────┐
│                 EvidenceIndexManager                     │
│  Coordinates all evidence indices and search operations  │
└────────┬────────────────────────────────────────────────┘
         │
         ├──> SymbolIndexImpl (SQLite FTS5)
         │    - Code symbols (functions, structs, traits)
         │    - Full-text search with metadata
         │
         ├──> TestIndexImpl (SQLite FTS5)
         │    - Test cases and test-to-symbol mappings
         │    - Test discovery and documentation
         │
         ├──> DocIndexImpl (SQLite FTS5)
         │    - READMEs, doc comments, ADRs
         │    - Documentation search and retrieval
         │
         └──> TenantIndex (Vector/HNSW)
              - Code chunks with embeddings
              - Semantic similarity search
```

### Per-Tenant Isolation

Each tenant gets isolated indices in `var/indices/{tenant_id}/`:
```
var/indices/
└── {tenant_id}/
    ├── symbols.db      # SQLite FTS5 index
    ├── tests.db        # SQLite FTS5 index
    ├── docs.db         # SQLite FTS5 index
    └── vectors/        # HNSW vector index
```

## Evidence Types

### 1. Symbol Evidence
- **What**: Functions, structs, traits, methods, types
- **Index**: SQLite FTS5 with metadata
- **Searchable**: Name, signature, docstring, module path
- **Use Case**: "Find the authenticate function"

### 2. Test Evidence
- **What**: Test cases, test functions, assertions
- **Index**: SQLite FTS5 with test-to-symbol mapping
- **Searchable**: Test name, target function, file path
- **Use Case**: "Show tests for authentication"

### 3. Documentation Evidence
- **What**: READMEs, doc comments, ADRs, markdown files
- **Index**: SQLite FTS5 full-text
- **Searchable**: Title, content, doc type
- **Use Case**: "Find documentation about JWT tokens"

### 4. Code Evidence
- **What**: Semantic code chunks (function-level)
- **Index**: Vector embeddings with HNSW
- **Searchable**: Semantic similarity
- **Use Case**: "Find code similar to user authentication"

### 5. Framework Evidence
- **What**: Framework-specific patterns and idioms
- **Index**: Combined doc + code search
- **Searchable**: Framework APIs, patterns
- **Use Case**: "Show React authentication patterns"

## Key Features

### Deterministic Retrieval

All evidence retrieval follows strict deterministic ordering:

```rust
// Results sorted by: (score DESC, doc_id ASC)
results.sort_by(|a, b| {
    b.score.partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.doc_id.cmp(&b.doc_id))
});
```

This ensures:
- Reproducible results across queries
- Audit trail consistency
- Policy compliance (Evidence Ruleset)

### Evidence Provenance

Every evidence span includes full provenance:

```rust
pub struct EvidenceSpan {
    pub doc_id: String,        // Unique document identifier
    pub rev: String,           // Version/revision
    pub span_hash: String,     // BLAKE3 hash of content
    pub score: f32,            // Relevance score
    pub evidence_type: EvidenceType,
    pub file_path: String,     // Source file
    pub start_line: usize,     // Line range
    pub end_line: usize,
    pub content: String,       // Actual evidence text
    pub metadata: HashMap<String, String>,
}
```

### Incremental Updates

The system supports incremental file-level updates:

```rust
// Handle file changes without full rebuild
manager.handle_file_changes(&[
    FileChange {
        path: PathBuf::from("src/auth.rs"),
        change_type: ChangeType::Modified,
        old_path: None,
    }
], "repo_id", "commit_sha").await?;
```

Supported change types:
- `Added`: Index new file
- `Modified`: Re-index changed file
- `Deleted`: Remove file indices
- `Renamed`: Remove old + index new

### Code Chunking

Code is chunked by semantic boundaries using tree-sitter:

**Chunking Strategy:**
- Function boundaries (primary)
- Struct/class boundaries
- Module boundaries
- Include context: imports, parent symbols
- Configurable chunk size with overlap

**Configuration:**
```rust
ChunkConfig {
    target_size: 1000,      // Target chars
    max_size: 2000,         // Max chars
    overlap: 200,           // Overlap chars
    include_context: true,  // Include imports/context
}
```

## Usage

### Creating Evidence Manager

```rust
use adapteros_lora_rag::EvidenceIndexManager;
use std::path::PathBuf;

// Create manager for a tenant
let manager = EvidenceIndexManager::new(
    PathBuf::from("var/indices"),
    "tenant_id".to_string(),
    Some(embedding_model),
).await?;
```

### Indexing a Repository

```rust
// Index entire repository
let stats = manager.index_repository(
    Path::new("/path/to/repo"),
    "repo_id"
).await?;

println!("Indexed {} symbols, {} tests, {} docs, {} code chunks",
    stats.symbols_indexed,
    stats.tests_indexed,
    stats.docs_indexed,
    stats.chunks_indexed
);
```

### Searching Evidence

```rust
use adapteros_lora_rag::EvidenceType;

// Search across multiple evidence types
let results = manager.search_evidence(
    "authenticate user",
    &[EvidenceType::Symbol, EvidenceType::Test, EvidenceType::Code],
    Some("repo_id"),
    10  // max results
).await?;

for span in results {
    println!("{}: {} (score: {:.2})",
        span.evidence_type,
        span.file_path,
        span.score
    );
}
```

### Worker Integration

```rust
use adapteros_lora_worker::evidence::EvidenceRetriever;

// Create retriever with evidence manager
let retriever = EvidenceRetriever::new(
    Arc::new(Mutex::new(evidence_manager))
);

// Retrieve patch evidence
let result = retriever.retrieve_patch_evidence(
    &EvidenceRequest {
        query: "authentication logic".to_string(),
        target_files: vec![],
        repo_id: "my_repo".to_string(),
        commit_sha: Some("abc123".to_string()),
        max_results: 10,
        min_score: 0.7,
    },
    "tenant_id"
).await?;
```

## Index Schema

### Symbol Index (symbols.db)

**FTS5 Table:**
```sql
CREATE VIRTUAL TABLE symbols_fts USING fts5(
    symbol_id UNINDEXED,
    name,                    -- Searchable: symbol name
    file_path UNINDEXED,
    start_line UNINDEXED,
    end_line UNINDEXED,
    kind,                    -- Searchable: symbol kind
    signature,               -- Searchable: function signature
    visibility,
    repo_id UNINDEXED,
    commit_sha UNINDEXED,
    docstring,               -- Searchable: documentation
    module_path,             -- Searchable: module path
    tokenize = 'porter unicode61'
);
```

**Metadata Table:**
```sql
CREATE TABLE symbols_metadata (
    symbol_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    last_updated TEXT NOT NULL,
    file_hash TEXT NOT NULL
);
```

### Test Index (tests.db)

**FTS5 Table:**
```sql
CREATE VIRTUAL TABLE tests_fts USING fts5(
    test_id UNINDEXED,
    test_name,               -- Searchable: test name
    file_path UNINDEXED,
    start_line UNINDEXED,
    end_line UNINDEXED,
    target_symbol_id UNINDEXED,
    target_function,         -- Searchable: function under test
    repo_id UNINDEXED,
    commit_sha UNINDEXED,
    tokenize = 'porter unicode61'
);
```

### Doc Index (docs.db)

**FTS5 Table:**
```sql
CREATE VIRTUAL TABLE docs_fts USING fts5(
    doc_id UNINDEXED,
    doc_type,                -- Searchable: README, ADR, etc.
    file_path UNINDEXED,
    title,                   -- Searchable: document title
    content,                 -- Searchable: full content
    repo_id UNINDEXED,
    commit_sha UNINDEXED,
    start_line UNINDEXED,
    end_line UNINDEXED,
    tokenize = 'porter unicode61'
);
```

## Policy Compliance

### Evidence Ruleset

The system enforces the Evidence Ruleset policy pack:

✅ **Mandatory Evidence Grounding**
- All inference responses include evidence spans
- Evidence spans have full provenance (doc_id, rev, span_hash)

✅ **Per-Tenant Isolation**
- Separate SQLite databases per tenant
- No cross-tenant evidence leakage

✅ **Deterministic Retrieval**
- Consistent ordering: (score desc, doc_id asc)
- Reproducible results for audit trails

✅ **Evidence Quality Metrics**
- ARR (Answer Relevance Rate) tracking
- ECS@5 (Evidence Citation Score at 5) tracking
- Configurable score thresholds

### Telemetry Integration

Evidence retrieval events are logged:

```rust
TelemetryWriter::log("evidence_retrieval", json!({
    "query": request.query,
    "evidence_types": evidence_types,
    "total_found": result.total_found,
    "retrieval_time_ms": result.retrieval_time_ms,
    "sources_used": result.sources_used,
    "tenant_id": tenant_id,
    "repo_id": request.repo_id,
}));
```

## Performance Characteristics

### FTS5 Search
- **Latency**: <10ms for typical queries
- **Throughput**: 1000+ QPS per index
- **Index Size**: ~100-200 bytes per symbol

### Vector Search
- **Latency**: <20ms for k=10 retrieval
- **Throughput**: 500+ QPS
- **Index Size**: Depends on embedding dimension

### Incremental Updates
- **File Update**: <100ms
- **Batch Updates**: ~10-50 files/second
- **Full Rebuild**: Minutes for 100K LOC

## Testing

Run comprehensive integration tests:

```bash
# Run all evidence tests
cargo test -p adapteros-lora-rag --test evidence_integration

# Run specific test
cargo test -p adapteros-lora-rag --test evidence_integration \
    test_symbol_index_create_and_search

# Run with output
cargo test -p adapteros-lora-rag --test evidence_integration -- --nocapture
```

## Troubleshooting

### Database Locked

**Symptom**: `database is locked` errors

**Solution**: Ensure SQLite connection pool is properly sized:
```rust
// Use connection limits
sqlx::sqlite::SqlitePoolOptions::new()
    .max_connections(5)
    .connect(&connection_string).await?
```

### Slow Searches

**Symptom**: Evidence retrieval >100ms

**Solutions**:
1. Check index statistics: `manager.get_stats().await?`
2. Rebuild indices if fragmented
3. Reduce max_results parameter
4. Add file path filters to narrow search

### Missing Evidence

**Symptom**: Expected evidence not returned

**Debugging**:
```rust
// Check index counts
let stats = manager.get_stats().await?;
println!("Symbols: {}, Tests: {}, Docs: {}",
    stats["symbols"], stats["tests"], stats["docs"]);

// Search with low threshold
let results = manager.search_evidence(
    query, types, repo_id, 100
).await?;
```

## Future Enhancements

### Planned Features

1. **Parallel Index Search**
   - Concurrent FTS5 + vector searches
   - Estimated speedup: 2-3x

2. **Incremental Embeddings**
   - Cache embeddings for unchanged code
   - Reduce re-indexing overhead

3. **Cross-Repository Search**
   - Search across multiple repos
   - Workspace-level indices

4. **Advanced Ranking**
   - BM25 for FTS5 ranking
   - Learned ranking models
   - Personalized relevance

5. **Evidence Compression**
   - Compressed evidence spans
   - Reduced network overhead

## References

- **Implementation**: `crates/adapteros-lora-rag/`
- **Tests**: `crates/adapteros-lora-rag/tests/evidence_integration.rs`
- **Worker Integration**: `crates/adapteros-lora-worker/src/evidence.rs`
- **Policy**: `docs/POLICIES.md` (Evidence Ruleset)
- **SQLite FTS5**: https://www.sqlite.org/fts5.html
- **HNSW**: https://arxiv.org/abs/1603.09320
