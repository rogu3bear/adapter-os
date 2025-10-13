# Code Intelligence Dependencies

## Overview

All dependencies required for code intelligence features, with version pinning for determinism.

---

## Core Dependencies

### Tree-sitter (Parsing)

**Primary**:
```toml
tree-sitter = "0.20.10"
```

**Language Grammars**:
```toml
tree-sitter-python = "0.20.4"
tree-sitter-rust = "0.20.4"
tree-sitter-typescript = "0.20.5"
tree-sitter-javascript = "0.20.3"
tree-sitter-go = "0.19.1"
tree-sitter-java = "0.19.2"
tree-sitter-c = "0.20.6"
tree-sitter-cpp = "0.20.3"
```

**Rationale**: Tree-sitter provides fast, incremental, and error-tolerant parsing with stable APIs.

**Determinism**: Pin exact versions; queries must match grammar version.

---

## Database & Indexing

### SQLite FTS5 (Symbol Index)

```toml
rusqlite = { version = "0.30.0", features = ["bundled", "modern_sqlite"] }
```

**Features**:
- `bundled`: Use bundled SQLite (no system dependency)
- `modern_sqlite`: Enable FTS5 and JSON functions

**Rationale**: SQLite FTS5 provides fast full-text search with deterministic results.

---

### Tantivy (Alternative Full-Text Search)

```toml
tantivy = "0.21.1"
```

**Use case**: Optional alternative to SQLite FTS5 for larger repos (>100K symbols).

**Note**: Not required for MVP; can be added later.

---

## Vector Search

### HNSW Implementation

```toml
hnsw = "0.11.0"
```

**Alternative** (if hnsw crate insufficient):
```rust
// Implement custom HNSW or use:
instant-distance = "0.6.0"
```

**Rationale**: HNSW provides fast approximate nearest neighbor search with good recall.

---

### Embedding Model

**Option 1: sentence-transformers (via Python FFI)**
- Model: `all-MiniLM-L6-v2`
- Dimension: 384
- Method: Call Python via subprocess or PyO3

```toml
pyo3 = { version = "0.20", features = ["auto-initialize"], optional = true }
```

**Option 2: Pure Rust (via Candle)**
```toml
candle-core = "0.3.0"
candle-nn = "0.3.0"
candle-transformers = "0.3.0"
```

**Recommended**: PyO3 for MVP (faster setup), migrate to Candle for production.

---

## Serialization & Hashing

```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"
blake3 = "1.5"
```

**Rationale**:
- `serde`: Standard Rust serialization
- `bincode`: Fast binary format for CodeGraph
- `blake3`: Fast, secure hashing for content-addressing

---

## Compression

```toml
zstd = "0.13"
tar = "0.4"
```

**Use case**: Bundle compression for artifacts (`.tar.zst` format).

---

## Async Runtime & HTTP

```toml
tokio = { version = "1.35", features = ["full"] }
axum = "0.7"
hyper = "1.0"
```

**Rationale**: Existing AdapterOS dependencies; reuse for job queue and API.

---

## Regex & Pattern Matching

```toml
regex = "1.10"
glob = "0.3"
```

**Use case**: Secret detection patterns, path allowlist/denylist.

---

## Git Integration

```toml
git2 = "0.18"
```

**Use case**: Reading commit metadata, diffs, and repo state.

**Alternative**: Shell out to `git` binary for simplicity.

---

## Testing Dependencies

```toml
[dev-dependencies]
tempfile = "3.8"
criterion = "0.5"
proptest = "1.4"
```

**Use case**:
- `tempfile`: Temporary directories for tests
- `criterion`: Benchmarking
- `proptest`: Property-based testing for determinism

---

## Optional Dependencies

### Language-Specific Tools (for evaluation)

**Python**:
```bash
uv pip install mypy ruff pytest
```

**Rust**:
```bash
cargo install rustfmt clippy
```

**TypeScript**:
```bash
pnpm add -D typescript eslint
```

**Use case**: Running compile/lint checks during evaluation.

---

## Version Pinning Strategy

### For Determinism

All dependencies with semantic impact must be pinned:

```toml
[dependencies]
tree-sitter = "=0.20.10"              # Exact version
tree-sitter-python = "=0.20.4"
rusqlite = { version = "=0.30.0", features = ["bundled"] }
blake3 = "=1.5.0"
```

### For Flexibility

Non-semantic dependencies can use caret requirements:

```toml
thiserror = "^1.0"
tracing = "^0.1"
```

---

## Cargo Features

### aos-codegraph

```toml
[features]
default = ["python", "rust", "typescript"]
python = ["tree-sitter-python"]
rust = ["tree-sitter-rust"]
typescript = ["tree-sitter-typescript"]
javascript = ["tree-sitter-javascript"]
go = ["tree-sitter-go"]
java = ["tree-sitter-java"]
all-languages = ["python", "rust", "typescript", "javascript", "go", "java"]
```

**Build example**:
```bash
cargo build --features all-languages
```

### aos-codepolicy

```toml
[features]
default = []
extra-patterns = []  # Additional secret patterns
```

---

## Dependency Graph

```
aos-codegraph
  ├── tree-sitter (core parsing)
  ├── tree-sitter-{python,rust,...} (language grammars)
  ├── rusqlite (symbol index)
  ├── hnsw (vector index)
  ├── blake3 (hashing)
  └── bincode (serialization)

aos-codepolicy
  ├── aos-policy (base policies)
  ├── regex (patterns)
  └── glob (path matching)

aos-codejobs
  ├── aos-codegraph
  ├── aos-artifacts (CAS storage)
  ├── aos-registry (metadata)
  ├── tokio (async runtime)
  └── git2 (git operations)

aos-codeapi
  ├── aos-codegraph
  ├── aos-codepolicy
  ├── aos-codejobs
  ├── axum (HTTP server)
  └── serde (JSON)
```

---

## Build Time Estimates

| Crate with dependencies | Clean build | Incremental |
|-------------------------|-------------|-------------|
| aos-codegraph           | ~45s        | ~3s         |
| aos-codepolicy          | ~15s        | ~2s         |
| aos-codejobs            | ~30s        | ~3s         |
| aos-codeapi             | ~35s        | ~3s         |
| **Total (first time)**  | **~125s**   | **~11s**    |

---

## Binary Size

| Crate             | Size (release) | Size (stripped) |
|-------------------|----------------|-----------------|
| aos-codegraph     | ~12 MB         | ~8 MB           |
| aos-codepolicy    | ~3 MB          | ~2 MB           |
| aos-codejobs      | ~8 MB          | ~5 MB           |
| aos-codeapi       | ~10 MB         | ~7 MB           |
| **Total added**   | **~33 MB**     | **~22 MB**      |

---

## Platform Support

### macOS (Primary Target)
- ✅ All dependencies supported
- ✅ Metal for GPU acceleration (existing)
- ✅ Secure Enclave (existing)

### Linux (Secondary)
- ✅ All dependencies supported
- ⚠️ No Metal (CPU-only or CUDA alternative)
- ⚠️ No Secure Enclave (use software keys)

### Windows
- ⚠️ Not officially supported (would require testing)

---

## Licensing

All dependencies are permissively licensed:

| Dependency              | License       |
|-------------------------|---------------|
| tree-sitter             | MIT           |
| tree-sitter-*           | MIT           |
| rusqlite                | MIT           |
| hnsw                    | Apache-2.0    |
| blake3                  | Apache-2.0 / CC0 |
| serde                   | MIT / Apache-2.0 |
| tokio                   | MIT           |
| axum                    | MIT           |
| regex                   | MIT / Apache-2.0 |

No GPL or AGPL dependencies. Compatible with AdapterOS dual MIT/Apache-2.0 license.

---

## Security Considerations

### Supply Chain

- All crates pulled from crates.io
- Use `cargo-deny` for audit (existing)
- Pin versions to prevent unexpected updates
- Review dependencies with `cargo-tree`

### Sandboxing

- Tree-sitter parsers are memory-safe (Rust)
- SQLite bundled (no system dependency attack surface)
- No network access during indexing
- File access limited by tenant isolation

---

## Performance Targets

With these dependencies, target performance:

- Parse 10K LOC: <5s (tree-sitter)
- Build CodeGraph: <10s (50K LOC repo)
- Symbol search: <10ms (SQLite FTS5)
- Vector search: <100ms (HNSW, 384-dim)
- Serialize CodeGraph: <1s (bincode)

---

## Future Optimizations

### Phase 2+

1. **Replace PyO3 with Candle**
   - Pure Rust embedding model
   - Faster startup, no Python dependency

2. **Custom HNSW**
   - Optimize for code search patterns
   - Better memory layout for Metal

3. **Incremental Parsing**
   - Use tree-sitter incremental mode
   - Only re-parse changed files

4. **Parallel Indexing**
   - Use Rayon for file-level parallelism
   - Build indices concurrently

---

## Installation

### Development

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install optional Python tools (for evaluation)
uv pip install mypy ruff pytest

# Build code intelligence crates
cargo build --package aos-codegraph --features all-languages
cargo build --package aos-codepolicy
cargo build --package aos-codejobs
cargo build --package aos-codeapi
```

### Production

```bash
# Release build with optimizations
cargo build --release --features all-languages

# Strip binaries
strip target/release/aosctl
```

---

## Dependency Updates

### Update Policy

- Security patches: Apply immediately
- Minor versions: Review and test
- Major versions: Planned migration, test determinism

### Testing After Update

```bash
# Run determinism tests
cargo test --test determinism

# Run code intelligence tests
cargo test --package aos-codegraph
cargo test --package aos-codepolicy
cargo test --package aos-codejobs
cargo test --package aos-codeapi

# Full audit
aosctl code-audit --corpus tests/corpora/code_eval_v1.json --cpid <cpid>
```
