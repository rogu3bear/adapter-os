# Code Ingestion Pipeline

## Overview

The ingestion pipeline transforms a repository into queryable artifacts: CodeGraph, symbol indices, vector indices, and test maps. All artifacts are deterministic, content-addressed, and stored in the CAS.

## Pipeline Stages

```
Repository
    │
    ├──> [1] Scan & Parse (tree-sitter)
    │         └──> Parsed ASTs per file
    │
    ├──> [2] Extract Symbols & Build Graph
    │         └──> CodeGraph (nodes + edges)
    │
    ├──> [3] Detect Frameworks
    │         └──> frameworks.json
    │
    ├──> [4] Build Symbol Index
    │         └──> SQLite FTS5 database
    │
    ├──> [5] Chunk & Embed
    │         └──> Vector index (HNSW)
    │
    ├──> [6] Map Tests
    │         └──> test_map.json
    │
    └──> [7] Package & Store
              └──> CAS artifacts + registry entries
```

## Stage 1: Scan & Parse

### Input
- Repository path
- Commit SHA
- Language hints (optional)

### Process

```rust
pub async fn scan_and_parse(
    repo_path: &Path,
    commit: &str,
    languages: &[Language],
) -> Result<ParsedRepo> {
    let mut parsed_files = Vec::new();
    
    // Walk directory tree
    for entry in WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored(e))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        
        let path = entry.path();
        let lang = detect_language(path)?;
        
        if !languages.contains(&lang) {
            continue;
        }
        
        // Parse with tree-sitter
        let source = std::fs::read_to_string(path)?;
        let parser = get_parser(lang)?;
        let tree = parser.parse(&source)?;
        
        parsed_files.push(ParsedFile {
            path: path.to_path_buf(),
            language: lang,
            source,
            tree,
            hash: blake3::hash(source.as_bytes()),
        });
    }
    
    Ok(ParsedRepo {
        repo_path: repo_path.to_path_buf(),
        commit: commit.to_string(),
        files: parsed_files,
    })
}

fn is_ignored(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    
    // Ignore common non-source directories
    IGNORED_DIRS.contains(&name.as_ref())
}

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    "target",
    "build",
    "dist",
    ".venv",
    "venv",
];
```

### Output
- Parsed AST per file
- File metadata (path, language, hash)

---

## Stage 2: Extract Symbols & Build Graph

### Process

```rust
pub fn build_code_graph(parsed: ParsedRepo) -> Result<CodeGraph> {
    let mut graph = CodeGraph::new(parsed.repo_path, parsed.commit);
    
    // Phase 1: Extract symbols from each file
    for file in &parsed.files {
        let file_node = graph.add_file_node(file)?;
        
        let symbols = extract_symbols(file)?;
        for symbol in symbols {
            graph.add_symbol_node(file_node.id, symbol)?;
        }
    }
    
    // Phase 2: Build relationships
    for file in &parsed.files {
        // Extract imports
        let imports = extract_imports(file)?;
        for import in imports {
            graph.add_import_edge(&import)?;
        }
        
        // Extract calls
        let calls = extract_calls(file)?;
        for call in calls {
            graph.add_call_edge(&call)?;
        }
        
        // Extract inheritance
        let inherits = extract_inheritance(file)?;
        for inherit in inherits {
            graph.add_inherit_edge(&inherit)?;
        }
    }
    
    // Phase 3: Validate and optimize
    graph.validate()?;
    graph.optimize_layout();
    
    Ok(graph)
}
```

### Symbol Extraction (Python example)

```rust
fn extract_symbols(file: &ParsedFile) -> Result<Vec<SymbolNode>> {
    let mut cursor = QueryCursor::new();
    let query = get_query(file.language, "symbols")?;
    
    let matches = cursor.matches(&query, file.tree.root_node(), file.source.as_bytes());
    
    let mut symbols = Vec::new();
    
    for m in matches {
        let capture_names: HashMap<_, _> = m.captures
            .iter()
            .map(|c| (query.capture_names()[c.index as usize], c.node))
            .collect();
        
        if let Some(&func_node) = capture_names.get("function.name") {
            let symbol = SymbolNode {
                id: generate_symbol_id(file, func_node),
                name: node_text(func_node, &file.source),
                kind: SymbolKind::Function,
                signature: extract_signature(capture_names.get("function.params"), &file.source),
                docstring: extract_docstring(capture_names.get("function.body"), &file.source),
                span: node_to_span(func_node),
                visibility: infer_visibility(&func_node, &file.source),
            };
            symbols.push(symbol);
        }
        
        // Similar for classes, methods, etc.
    }
    
    Ok(symbols)
}
```

### Output
- CodeGraph with nodes and edges
- Symbol count, file count

---

## Stage 3: Detect Frameworks

### Process

```rust
pub fn detect_frameworks(repo_path: &Path) -> Result<Vec<Framework>> {
    let mut detected = Vec::new();
    
    for fingerprint in FRAMEWORK_FINGERPRINTS {
        if fingerprint.matches(repo_path)? {
            let version = fingerprint.extract_version(repo_path)?;
            let config_files = fingerprint.collect_config_files(repo_path)?;
            
            detected.push(Framework {
                name: fingerprint.name.clone(),
                version,
                config_files,
            });
        }
    }
    
    // Deduplicate and sort
    detected.sort_by(|a, b| a.name.cmp(&b.name));
    detected.dedup_by(|a, b| a.name == b.name);
    
    Ok(detected)
}
```

### Framework Fingerprints

**Django**:
- Required: `manage.py`, `settings.py` (or `*/settings.py`)
- Version: From `requirements.txt` or `pyproject.toml`

**React**:
- Required: `package.json` with `"react"` dependency
- Version: From `package.json`

**FastAPI**:
- Required: `main.py` or `app.py` with `from fastapi import`
- Version: From `requirements.txt`

**pytest**:
- Required: `pytest.ini` or `pyproject.toml` with `[tool.pytest]`
- Version: From `requirements.txt`

### Output
- `frameworks.json`: List of detected frameworks with versions

---

## Stage 4: Build Symbol Index

### Process

```rust
pub fn build_symbol_index(graph: &CodeGraph) -> Result<SymbolIndex> {
    let index_path = temp_sqlite_path();
    let conn = Connection::open(&index_path)?;
    
    // Create FTS5 table
    conn.execute(
        "CREATE VIRTUAL TABLE symbols USING fts5(
            symbol_id UNINDEXED,
            name,
            kind,
            signature,
            docstring,
            file_path,
            content='symbols_data',
            content_rowid='rowid'
        )",
        [],
    )?;
    
    conn.execute(
        "CREATE TABLE symbols_data (
            rowid INTEGER PRIMARY KEY,
            symbol_id TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            signature TEXT,
            docstring TEXT,
            file_path TEXT NOT NULL,
            span_json TEXT NOT NULL
        )",
        [],
    )?;
    
    // Insert all symbols
    let mut stmt = conn.prepare(
        "INSERT INTO symbols_data (symbol_id, name, kind, signature, docstring, file_path, span_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )?;
    
    for symbol in graph.symbols() {
        let file_path = graph.file_path(symbol.file_id)?;
        stmt.execute(params![
            symbol.id.to_string(),
            symbol.name,
            symbol.kind.to_string(),
            symbol.signature,
            symbol.docstring,
            file_path.display().to_string(),
            serde_json::to_string(&symbol.span)?,
        ])?;
    }
    
    // Rebuild FTS5 index
    conn.execute("INSERT INTO symbols(symbols) VALUES('rebuild')", [])?;
    
    // Compute hash
    let index_bytes = std::fs::read(&index_path)?;
    let hash = blake3::hash(&index_bytes);
    
    Ok(SymbolIndex {
        path: index_path,
        hash: hash.into(),
        symbol_count: graph.symbols().count(),
    })
}
```

### Output
- SQLite database with FTS5 index
- Hash: `b3(index_file_bytes)`

---

## Stage 5: Chunk & Embed

### Chunking Strategy

```rust
pub fn chunk_code_files(graph: &CodeGraph) -> Result<Vec<CodeChunk>> {
    let mut chunks = Vec::new();
    
    for file in graph.files() {
        let source = std::fs::read_to_string(&file.path)?;
        let lines: Vec<&str> = source.lines().collect();
        
        // Symbol-aware chunking
        for symbol in graph.symbols_in_file(file.id) {
            let span = &symbol.span;
            let start = span.start_line as usize;
            let end = span.end_line as usize;
            
            // Include context: 5 lines before/after
            let context_start = start.saturating_sub(5);
            let context_end = (end + 5).min(lines.len());
            
            let chunk_text = lines[context_start..context_end].join("\n");
            
            chunks.push(CodeChunk {
                chunk_id: generate_chunk_id(file.id, symbol.id),
                file_id: file.id,
                symbol_id: Some(symbol.id),
                text: chunk_text,
                span: Span {
                    start_line: context_start as u32,
                    start_col: 0,
                    end_line: context_end as u32,
                    end_col: 0,
                },
                metadata: ChunkMetadata {
                    language: file.language,
                    symbol_kind: Some(symbol.kind),
                    is_test: file.metadata.is_test,
                },
            });
        }
        
        // For files without symbols (config, markdown), chunk by line count
        if graph.symbols_in_file(file.id).count() == 0 {
            for (i, window) in lines.chunks(50).enumerate() {
                chunks.push(CodeChunk {
                    chunk_id: generate_chunk_id(file.id, ChunkOffset(i)),
                    file_id: file.id,
                    symbol_id: None,
                    text: window.join("\n"),
                    span: Span {
                        start_line: (i * 50) as u32,
                        start_col: 0,
                        end_line: ((i + 1) * 50).min(lines.len()) as u32,
                        end_col: 0,
                    },
                    metadata: ChunkMetadata {
                        language: file.language,
                        symbol_kind: None,
                        is_test: file.metadata.is_test,
                    },
                });
            }
        }
    }
    
    Ok(chunks)
}
```

### Embedding

```rust
pub fn embed_chunks(chunks: &[CodeChunk], model: &EmbeddingModel) -> Result<Vec<f32>> {
    let mut embeddings = Vec::with_capacity(chunks.len() * model.dim);
    
    for chunk in chunks {
        let emb = model.embed(&chunk.text)?;
        embeddings.extend_from_slice(&emb);
    }
    
    Ok(embeddings)
}
```

### Build HNSW Index

```rust
pub fn build_vector_index(
    chunks: &[CodeChunk],
    embeddings: &[f32],
    dim: usize,
) -> Result<VectorIndex> {
    let mut hnsw = Hnsw::new(dim, chunks.len(), 16, 200, DistanceMetric::Cosine);
    
    for (i, chunk) in chunks.iter().enumerate() {
        let start = i * dim;
        let end = start + dim;
        let vector = &embeddings[start..end];
        
        hnsw.insert(i, vector)?;
    }
    
    // Serialize
    let serialized = hnsw.serialize()?;
    let hash = blake3::hash(&serialized);
    
    Ok(VectorIndex {
        data: serialized,
        hash: hash.into(),
        dim,
        count: chunks.len(),
    })
}
```

### Output
- Vector index (HNSW)
- Chunk metadata JSON
- Hash: `b3(index_bytes)`

---

## Stage 6: Map Tests

### Process

```rust
pub fn map_tests(graph: &CodeGraph) -> Result<TestMap> {
    let mut file_coverage: HashMap<FileId, Vec<TestId>> = HashMap::new();
    let mut symbol_coverage: HashMap<SymbolId, Vec<TestId>> = HashMap::new();
    
    for test in graph.tests() {
        // Direct coverage: explicit test_covers edges
        for symbol_id in graph.test_targets(test.id) {
            symbol_coverage
                .entry(symbol_id)
                .or_default()
                .push(test.id);
            
            let file_id = graph.file_for_symbol(symbol_id)?;
            file_coverage
                .entry(file_id)
                .or_default()
                .push(test.id);
        }
        
        // Indirect coverage: 1-hop from test file
        let test_file = graph.file_for_symbol(test.id)?;
        for import_edge in graph.imports_from(test_file) {
            let target_file = import_edge.to;
            file_coverage
                .entry(target_file)
                .or_default()
                .push(test.id);
            
            for symbol in graph.symbols_in_file(target_file) {
                symbol_coverage
                    .entry(symbol.id)
                    .or_default()
                    .push(test.id);
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
    
    Ok(TestMap {
        file_coverage,
        symbol_coverage,
    })
}
```

### Output
- `test_map.json`: File/symbol → tests mapping
- Hash: `b3(json_bytes)`

---

## Stage 7: Package & Store

### Process

```rust
pub async fn package_and_store(
    repo_id: &str,
    commit: &str,
    graph: CodeGraph,
    symbol_index: SymbolIndex,
    vector_index: VectorIndex,
    test_map: TestMap,
    cas: &CasStore,
    registry: &Registry,
) -> Result<IngestResult> {
    // 1. Store CodeGraph
    let graph_binary = graph.to_binary()?;
    let graph_hash = cas.put(&graph_binary).await?;
    
    registry.store_code_graph(&CodeGraphRecord {
        id: format!("graph_{}_{}", repo_id, &commit[..8]),
        repo_id: repo_id.to_string(),
        commit_sha: commit.to_string(),
        hash_b3: graph_hash,
        file_count: graph.files().count(),
        symbol_count: graph.symbols().count(),
        test_count: graph.tests().count(),
        languages: graph.languages(),
        frameworks: graph.frameworks(),
        size_bytes: graph_binary.len(),
    }).await?;
    
    // 2. Store symbol index
    let symbol_index_bytes = std::fs::read(&symbol_index.path)?;
    let symbol_hash = cas.put(&symbol_index_bytes).await?;
    
    registry.store_symbol_index(&SymbolIndexRecord {
        id: format!("symidx_{}_{}", repo_id, &commit[..8]),
        code_graph_id: graph_hash.to_string(),
        index_type: "sqlite_fts5".to_string(),
        hash_b3: symbol_hash,
        symbol_count: symbol_index.symbol_count,
        size_bytes: symbol_index_bytes.len(),
    }).await?;
    
    // 3. Store vector index
    let vector_hash = cas.put(&vector_index.data).await?;
    
    registry.store_vector_index(&VectorIndexRecord {
        id: format!("vecidx_{}_{}", repo_id, &commit[..8]),
        code_graph_id: graph_hash.to_string(),
        embedding_model: "all-MiniLM-L6-v2".to_string(),
        embedding_dim: vector_index.dim,
        chunk_count: vector_index.count,
        hash_b3: vector_hash,
        size_bytes: vector_index.data.len(),
    }).await?;
    
    // 4. Store test map
    let test_map_json = serde_json::to_vec(&test_map)?;
    let test_map_hash = cas.put(&test_map_json).await?;
    
    registry.store_test_map(&TestMapRecord {
        id: format!("testmap_{}_{}", repo_id, &commit[..8]),
        code_graph_id: graph_hash.to_string(),
        hash_b3: test_map_hash,
        test_count: test_map.file_coverage.len(),
        file_coverage_json: serde_json::to_string(&test_map.file_coverage)?,
        symbol_coverage_json: serde_json::to_string(&test_map.symbol_coverage)?,
    }).await?;
    
    // 5. Update repository latest_scan
    registry.update_repo_scan(repo_id, commit).await?;
    
    Ok(IngestResult {
        code_graph_hash: graph_hash,
        symbol_index_hash: symbol_hash,
        vector_index_hash: vector_hash,
        test_map_hash: test_map_hash,
    })
}
```

---

## CLI Usage

### Full Ingestion

```bash
aosctl code-init \
  --tenant acme \
  --repo acme/payments \
  --path /repos/acme/payments \
  --commit $(git -C /repos/acme/payments rev-parse HEAD) \
  --languages python,typescript
```

### Incremental Update (Commit Delta)

```bash
aosctl code-update \
  --tenant acme \
  --repo acme/payments \
  --commit $NEW_COMMIT \
  --parent $PARENT_COMMIT
```

---

## Performance Targets

- **Scan & parse** (10K LOC): <10s
- **Build CodeGraph**: <5s
- **Symbol index**: <3s
- **Vector index** (with embedding): <30s (GPU), <120s (CPU)
- **Test map**: <2s
- **Total** (10K LOC): <60s (GPU), <150s (CPU)

Parallelization across files and stages reduces wall-clock time by ~3x.

---

## Determinism

All stages are deterministic:
- Tree-sitter parsing: stable for given grammar version
- Symbol extraction: query-based, no heuristics
- Framework detection: rule-based
- Graph serialization: sorted by IDs
- Hashing: BLAKE3 on canonical bytes

Given identical repo state and versions, two runs produce identical hashes.
