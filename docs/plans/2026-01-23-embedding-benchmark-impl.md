# Embedding Benchmark Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build deterministic embedding benchmark system with retrieval receipts, MLX training, and UI integration.

**Architecture:** Two new crates (`adapteros-embeddings`, `adapteros-retrieval`) building on existing MLX FFI and chunking infrastructure. CLI subcommands under `aosctl embed`. UI components in audit page and testing panel.

**Tech Stack:** Rust, MLX FFI, BLAKE3, Ed25519, Leptos (WASM), existing adapteros-* crates.

---

## Phase 1: Core Crates (Parallel)

### Task 1.1: Create adapteros-embeddings crate skeleton

**Files:**
- Create: `crates/adapteros-embeddings/Cargo.toml`
- Create: `crates/adapteros-embeddings/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "adapteros-embeddings"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
adapteros-core = { path = "../adapteros-core" }
adapteros-crypto = { path = "../adapteros-crypto" }
adapteros-config = { path = "../adapteros-config" }
adapteros-lora-mlx-ffi = { path = "../adapteros-lora-mlx-ffi", optional = true }

async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
blake3 = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
tokenizers = { workspace = true }

[features]
default = ["mlx"]
mlx = ["dep:adapteros-lora-mlx-ffi"]
training = []

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
tempfile = { workspace = true }
```

**Step 2: Create lib.rs with public API**

```rust
//! Deterministic embedding generation with model hashing and seed tracking
//!
//! Provides:
//! - EmbeddingModel trait for pluggable backends
//! - MLX implementation (production)
//! - Determinism verification

pub mod config;
pub mod determinism;
pub mod model;

#[cfg(feature = "training")]
pub mod lora;
#[cfg(feature = "training")]
pub mod training;

pub use model::{Embedding, EmbeddingModel, EmbeddingProvider};

// Re-export from MLX FFI when available
#[cfg(feature = "mlx")]
pub use adapteros_lora_mlx_ffi::MLXEmbeddingModel;
```

**Step 3: Add to workspace Cargo.toml**

Add `"crates/adapteros-embeddings"` to workspace members list.

**Step 4: Verify compilation**

Run: `cargo check -p adapteros-embeddings`
Expected: Compiles with warnings about empty modules

**Step 5: Commit**

```bash
git add crates/adapteros-embeddings/ Cargo.toml
git commit -m "feat(embeddings): create adapteros-embeddings crate skeleton"
```

---

### Task 1.2: Create adapteros-retrieval crate skeleton

**Files:**
- Create: `crates/adapteros-retrieval/Cargo.toml`
- Create: `crates/adapteros-retrieval/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "adapteros-retrieval"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
adapteros-core = { path = "../adapteros-core" }
adapteros-crypto = { path = "../adapteros-crypto" }
adapteros-embeddings = { path = "../adapteros-embeddings" }
adapteros-ingest-docs = { path = "../adapteros-ingest-docs" }
adapteros-lora-rag = { path = "../adapteros-lora-rag" }
adapteros-telemetry = { path = "../adapteros-telemetry" }

async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
blake3 = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
ed25519-dalek = { workspace = true }
hnsw_rs = { workspace = true }

[features]
default = []
hnsw = []

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
tempfile = { workspace = true }
```

**Step 2: Create lib.rs with public API**

```rust
//! Deterministic retrieval with receipts and benchmarking
//!
//! Provides:
//! - Hybrid chunking (token + semantic)
//! - Flat and HNSW index backends
//! - Retrieval receipts with Ed25519 signing
//! - Benchmark harness with eval metrics

pub mod benchmark;
pub mod chunking;
pub mod corpus;
pub mod eval;
pub mod index;
pub mod query_gen;
pub mod receipt;

pub use benchmark::{BenchmarkConfig, BenchmarkHarness, BenchmarkReport};
pub use corpus::{Chunk, ChunkType, Corpus};
pub use index::{IndexBackend, IndexMetadata, SearchResult};
pub use receipt::RetrievalReceipt;
```

**Step 3: Add to workspace Cargo.toml**

Add `"crates/adapteros-retrieval"` to workspace members list.

**Step 4: Verify compilation**

Run: `cargo check -p adapteros-retrieval`
Expected: Compiles with warnings about empty modules

**Step 5: Commit**

```bash
git add crates/adapteros-retrieval/ Cargo.toml
git commit -m "feat(retrieval): create adapteros-retrieval crate skeleton"
```

---

### Task 1.3: Implement EmbeddingModel trait and types

**Files:**
- Create: `crates/adapteros-embeddings/src/model.rs`
- Modify: `crates/adapteros-embeddings/src/lib.rs`

**Step 1: Write test for EmbeddingModel trait**

```rust
// In model.rs
#[cfg(test)]
mod tests {
    use super::*;

    struct MockModel {
        dim: usize,
        hash: B3Hash,
    }

    #[async_trait::async_trait]
    impl EmbeddingModel for MockModel {
        async fn embed(&self, _text: &str) -> Result<Embedding> {
            Ok(Embedding {
                vector: vec![0.0; self.dim],
                model_hash: self.hash,
                input_hash: B3Hash::hash(b"test"),
            })
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.embed(text).await?);
            }
            Ok(results)
        }

        fn model_hash(&self) -> &B3Hash { &self.hash }
        fn tokenizer_hash(&self) -> &B3Hash { &self.hash }
        fn embedding_dimension(&self) -> usize { self.dim }
    }

    #[tokio::test]
    async fn test_mock_model_embed() {
        let hash = B3Hash::hash(b"mock");
        let model = MockModel { dim: 384, hash };
        let emb = model.embed("hello").await.unwrap();
        assert_eq!(emb.vector.len(), 384);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p adapteros-embeddings test_mock_model_embed`
Expected: FAIL - types not defined

**Step 3: Implement types**

```rust
//! Embedding model trait and types

use adapteros_core::{B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A computed embedding with provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector (normalized)
    pub vector: Vec<f32>,
    /// Hash of the model that produced this embedding
    pub model_hash: B3Hash,
    /// Hash of the input text
    pub input_hash: B3Hash,
}

impl Embedding {
    /// Compute hash of the embedding vector for determinism checks
    pub fn vector_hash(&self) -> B3Hash {
        let bytes: Vec<u8> = self.vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        B3Hash::hash(&bytes)
    }

    /// Compute cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.vector.len() != other.vector.len() {
            return 0.0;
        }
        let dot: f32 = self.vector.iter().zip(&other.vector).map(|(a, b)| a * b).sum();
        let norm_a: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a < 1e-9 || norm_b < 1e-9 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }
}

/// Trait for embedding models with determinism guarantees
#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    /// Embed a single text, returning normalized vector
    async fn embed(&self, text: &str) -> Result<Embedding>;

    /// Embed batch for throughput
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// Model identity hash for determinism tracking
    fn model_hash(&self) -> &B3Hash;

    /// Tokenizer identity hash
    fn tokenizer_hash(&self) -> &B3Hash;

    /// Embedding dimension
    fn embedding_dimension(&self) -> usize;
}

/// Provider that wraps an embedding model with configuration
pub struct EmbeddingProvider {
    model: Box<dyn EmbeddingModel>,
}

impl EmbeddingProvider {
    /// Create provider with given model
    pub fn new(model: Box<dyn EmbeddingModel>) -> Self {
        Self { model }
    }

    /// Get underlying model
    pub fn model(&self) -> &dyn EmbeddingModel {
        self.model.as_ref()
    }

    /// Embed text
    pub async fn embed(&self, text: &str) -> Result<Embedding> {
        self.model.embed(text).await
    }

    /// Embed batch
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        self.model.embed_batch(texts).await
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p adapteros-embeddings test_mock_model_embed`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/adapteros-embeddings/src/model.rs
git commit -m "feat(embeddings): implement EmbeddingModel trait and Embedding type"
```

---

### Task 1.4: Implement Corpus and Chunk types

**Files:**
- Create: `crates/adapteros-retrieval/src/corpus.rs`
- Create: `crates/adapteros-retrieval/src/chunking.rs`

**Step 1: Write test for Corpus version hash**

```rust
// In corpus.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corpus_version_hash_deterministic() {
        let chunks = vec![
            Chunk {
                chunk_id: "a".to_string(),
                source_path: "test.rs".to_string(),
                content: "fn main() {}".to_string(),
                start_offset: 0,
                end_offset: 12,
                chunk_type: ChunkType::Code {
                    language: "rust".to_string(),
                    semantic_type: "function".to_string()
                },
                content_hash: B3Hash::hash(b"fn main() {}"),
            },
            Chunk {
                chunk_id: "b".to_string(),
                source_path: "test.md".to_string(),
                content: "# Hello".to_string(),
                start_offset: 0,
                end_offset: 7,
                chunk_type: ChunkType::Document { format: "markdown".to_string() },
                content_hash: B3Hash::hash(b"# Hello"),
            },
        ];

        let hash1 = Corpus::compute_version_hash(&chunks);
        let hash2 = Corpus::compute_version_hash(&chunks);
        assert_eq!(hash1, hash2);

        // Order shouldn't matter (sorted by chunk_id)
        let mut reversed = chunks.clone();
        reversed.reverse();
        let hash3 = Corpus::compute_version_hash(&reversed);
        assert_eq!(hash1, hash3);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p adapteros-retrieval test_corpus_version_hash`
Expected: FAIL - types not defined

**Step 3: Implement Corpus types**

```rust
//! Corpus management with deterministic versioning

use adapteros_core::B3Hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A chunk of content from the corpus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique chunk ID: blake3(source_path + start_offset + length)
    pub chunk_id: String,
    /// Source file path
    pub source_path: String,
    /// Chunk content
    pub content: String,
    /// Start offset in source
    pub start_offset: usize,
    /// End offset in source
    pub end_offset: usize,
    /// Chunk type (code or document)
    pub chunk_type: ChunkType,
    /// BLAKE3 hash of content
    pub content_hash: B3Hash,
}

impl Chunk {
    /// Generate deterministic chunk ID
    pub fn generate_id(source_path: &str, start_offset: usize, length: usize) -> String {
        let data = format!("{}:{}:{}", source_path, start_offset, length);
        B3Hash::hash(data.as_bytes()).to_hex()
    }

    /// Create new chunk with auto-generated ID
    pub fn new(
        source_path: String,
        content: String,
        start_offset: usize,
        end_offset: usize,
        chunk_type: ChunkType,
    ) -> Self {
        let length = end_offset - start_offset;
        let chunk_id = Self::generate_id(&source_path, start_offset, length);
        let content_hash = B3Hash::hash(content.as_bytes());
        Self {
            chunk_id,
            source_path,
            content,
            start_offset,
            end_offset,
            chunk_type,
            content_hash,
        }
    }
}

/// Type of chunk (for hybrid chunking strategy selection)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    /// Code chunk with language and semantic type
    Code {
        language: String,
        semantic_type: String, // function, class, module, etc.
    },
    /// Document chunk with format
    Document {
        format: String, // markdown, text, etc.
    },
}

/// Chunking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Token chunk size for documents
    pub token_chunk_size: usize,
    /// Token overlap for documents
    pub token_overlap: usize,
    /// Target size for code chunks (chars)
    pub code_target_size: usize,
    /// Max size for code chunks (chars)
    pub code_max_size: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            token_chunk_size: 512,
            token_overlap: 128,
            code_target_size: 1000,
            code_max_size: 2000,
        }
    }
}

/// A versioned corpus of chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Corpus {
    /// Unique corpus ID
    pub corpus_id: String,
    /// Deterministic version hash of all chunk hashes
    pub version_hash: B3Hash,
    /// All chunks in the corpus
    pub chunks: Vec<Chunk>,
    /// Chunking configuration used
    pub chunking_config: ChunkingConfig,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl Corpus {
    /// Compute deterministic version hash from chunks
    ///
    /// Sorts by chunk_id to ensure order-independent hashing
    pub fn compute_version_hash(chunks: &[Chunk]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Sort by chunk_id for determinism
        let mut sorted_ids: Vec<_> = chunks.iter()
            .map(|c| (&c.chunk_id, &c.content_hash))
            .collect();
        sorted_ids.sort_by_key(|(id, _)| *id);

        for (_, content_hash) in sorted_ids {
            hasher.update(content_hash.as_bytes());
        }

        B3Hash::from(hasher.finalize())
    }

    /// Create new corpus from chunks
    pub fn new(chunks: Vec<Chunk>, chunking_config: ChunkingConfig) -> Self {
        let version_hash = Self::compute_version_hash(&chunks);
        let corpus_id = uuid::Uuid::new_v4().to_string();
        Self {
            corpus_id,
            version_hash,
            chunks,
            chunking_config,
            created_at: Utc::now(),
        }
    }

    /// Number of chunks
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p adapteros-retrieval test_corpus_version_hash`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/adapteros-retrieval/src/corpus.rs
git commit -m "feat(retrieval): implement Corpus and Chunk types with deterministic hashing"
```

---

### Task 1.5: Implement IndexBackend trait and FlatIndex

**Files:**
- Create: `crates/adapteros-retrieval/src/index/mod.rs`
- Create: `crates/adapteros-retrieval/src/index/flat.rs`

**Step 1: Write test for FlatIndex determinism**

```rust
// In index/flat.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_flat_index_deterministic() {
        let mut index = FlatIndex::new();

        // Add some embeddings
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0, 0.0]),
            ("chunk2".to_string(), vec![0.0, 1.0, 0.0]),
            ("chunk3".to_string(), vec![0.7, 0.7, 0.0]),
        ];

        index.build(&embeddings).await.unwrap();

        // Query
        let query = vec![1.0, 0.0, 0.0];
        let results1 = index.search(&query, 2).await.unwrap();
        let results2 = index.search(&query, 2).await.unwrap();

        // Must be deterministic
        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(&results2) {
            assert_eq!(r1.chunk_id, r2.chunk_id);
            assert!((r1.score - r2.score).abs() < 1e-6);
        }

        // chunk1 should be first (exact match)
        assert_eq!(results1[0].chunk_id, "chunk1");
    }

    #[tokio::test]
    async fn test_flat_index_tiebreaking() {
        let mut index = FlatIndex::new();

        // Same score for multiple chunks
        let embeddings = vec![
            ("b_chunk".to_string(), vec![1.0, 0.0]),
            ("a_chunk".to_string(), vec![1.0, 0.0]),
            ("c_chunk".to_string(), vec![1.0, 0.0]),
        ];

        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0];
        let results = index.search(&query, 3).await.unwrap();

        // Tie-breaking: score DESC, chunk_id ASC
        assert_eq!(results[0].chunk_id, "a_chunk");
        assert_eq!(results[1].chunk_id, "b_chunk");
        assert_eq!(results[2].chunk_id, "c_chunk");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p adapteros-retrieval test_flat_index`
Expected: FAIL - types not defined

**Step 3: Implement IndexBackend trait (mod.rs)**

```rust
//! Index backends for vector search

pub mod flat;
#[cfg(feature = "hnsw")]
pub mod hnsw;

use adapteros_core::{B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use flat::FlatIndex;
#[cfg(feature = "hnsw")]
pub use hnsw::HnswIndex;

/// Search result with deterministic ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Chunk ID
    pub chunk_id: String,
    /// Similarity score (higher = more similar)
    pub score: f32,
    /// Rank (0 = best match)
    pub rank: usize,
}

/// Index metadata for receipts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    /// Index type ("flat" or "hnsw")
    pub index_type: String,
    /// Hash of build parameters
    pub params_hash: B3Hash,
    /// Build seed (for HNSW reproducibility)
    pub build_seed: Option<u64>,
    /// Number of vectors in index
    pub num_vectors: usize,
    /// Vector dimension
    pub dimension: usize,
}

/// Trait for vector index backends
#[async_trait]
pub trait IndexBackend: Send + Sync {
    /// Build index from (chunk_id, embedding) pairs
    async fn build(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<IndexMetadata>;

    /// Search for top-k nearest neighbors
    ///
    /// Results are ordered by:
    /// 1. score DESC
    /// 2. chunk_id ASC (tie-breaking)
    async fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>>;

    /// Get index metadata
    fn metadata(&self) -> &IndexMetadata;
}
```

**Step 4: Implement FlatIndex (flat.rs)**

```rust
//! Flat (brute-force) index for exact nearest neighbor search

use super::{IndexBackend, IndexMetadata, SearchResult};
use adapteros_core::{AosError, B3Hash, Result};
use async_trait::async_trait;
use std::cmp::Ordering;

/// Flat index using brute-force exact search
///
/// Guarantees:
/// - 100% recall (exact nearest neighbors)
/// - Deterministic results with tie-breaking
/// - O(n) search complexity
pub struct FlatIndex {
    /// Stored embeddings: (chunk_id, vector)
    embeddings: Vec<(String, Vec<f32>)>,
    /// Index metadata
    metadata: IndexMetadata,
}

impl FlatIndex {
    /// Create empty flat index
    pub fn new() -> Self {
        Self {
            embeddings: Vec::new(),
            metadata: IndexMetadata {
                index_type: "flat".to_string(),
                params_hash: B3Hash::hash(b"flat_index_v1"),
                build_seed: None,
                num_vectors: 0,
                dimension: 0,
            },
        }
    }

    /// Compute cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a < 1e-9 || norm_b < 1e-9 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }
}

impl Default for FlatIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IndexBackend for FlatIndex {
    async fn build(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<IndexMetadata> {
        if embeddings.is_empty() {
            return Err(AosError::Validation("Cannot build index with no embeddings".to_string()));
        }

        let dimension = embeddings[0].1.len();

        // Validate all have same dimension
        for (id, vec) in embeddings {
            if vec.len() != dimension {
                return Err(AosError::Validation(format!(
                    "Embedding {} has dimension {} but expected {}",
                    id, vec.len(), dimension
                )));
            }
        }

        self.embeddings = embeddings.to_vec();
        self.metadata = IndexMetadata {
            index_type: "flat".to_string(),
            params_hash: B3Hash::hash(b"flat_index_v1"),
            build_seed: None,
            num_vectors: embeddings.len(),
            dimension,
        };

        Ok(self.metadata.clone())
    }

    async fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }

        if query.len() != self.metadata.dimension {
            return Err(AosError::Validation(format!(
                "Query dimension {} doesn't match index dimension {}",
                query.len(), self.metadata.dimension
            )));
        }

        // Compute all similarities
        let mut scored: Vec<(String, f32)> = self.embeddings
            .iter()
            .map(|(id, vec)| (id.clone(), Self::cosine_similarity(query, vec)))
            .collect();

        // Sort by score DESC, then chunk_id ASC for deterministic tie-breaking
        scored.sort_by(|a, b| {
            match b.1.partial_cmp(&a.1) {
                Some(Ordering::Equal) | None => a.0.cmp(&b.0),
                Some(ord) => ord,
            }
        });

        // Take top-k
        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(top_k)
            .enumerate()
            .map(|(rank, (chunk_id, score))| SearchResult {
                chunk_id,
                score,
                rank,
            })
            .collect();

        Ok(results)
    }

    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p adapteros-retrieval test_flat_index`
Expected: PASS (both tests)

**Step 6: Commit**

```bash
git add crates/adapteros-retrieval/src/index/
git commit -m "feat(retrieval): implement FlatIndex with deterministic tie-breaking"
```

---

### Task 1.6: Implement RetrievalReceipt with signing

**Files:**
- Create: `crates/adapteros-retrieval/src/receipt.rs`

**Step 1: Write test for receipt digest determinism**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receipt_digest_deterministic() {
        let receipt = RetrievalReceipt {
            embedder_model_hash: B3Hash::hash(b"model"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            corpus_version_hash: B3Hash::hash(b"corpus"),
            chunking_params: ChunkingConfig::default(),
            index_type: "flat".to_string(),
            index_params_hash: B3Hash::hash(b"params"),
            index_seed: None,
            query_text_hash: B3Hash::hash(b"query"),
            query_embedding_hash: B3Hash::hash(b"embedding"),
            top_k: vec![
                ("chunk1".to_string(), 0.95),
                ("chunk2".to_string(), 0.87),
            ],
            seed_lineage: None,
            tenant_id: "test".to_string(),
            request_id: "req1".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-01-23T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            embed_latency_ms: 10.5,
            search_latency_ms: 5.2,
            signature: None,
        };

        let digest1 = receipt.compute_digest();
        let digest2 = receipt.compute_digest();
        assert_eq!(digest1, digest2);
    }
}
```

**Step 2: Implement RetrievalReceipt**

```rust
//! Retrieval receipts with cryptographic signing

use crate::corpus::ChunkingConfig;
use adapteros_core::{B3Hash, Result, SeedLineage};
use adapteros_crypto::receipt_signing::{KeyManager, SignedReceipt};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Full audit receipt for a retrieval operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalReceipt {
    // Model identity
    pub embedder_model_hash: B3Hash,
    pub tokenizer_hash: B3Hash,

    // Corpus identity
    pub corpus_version_hash: B3Hash,
    pub chunking_params: ChunkingConfig,

    // Index identity
    pub index_type: String,
    pub index_params_hash: B3Hash,
    pub index_seed: Option<u64>,

    // Query identity
    pub query_text_hash: B3Hash,
    pub query_embedding_hash: B3Hash,

    // Results (deterministic order: score DESC, chunk_id ASC)
    pub top_k: Vec<(String, f32)>,

    // Seed lineage (for replay verification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_lineage: Option<SeedLineage>,

    // Tenant context
    pub tenant_id: String,
    pub request_id: String,
    pub timestamp: DateTime<Utc>,

    // Metrics snapshot
    pub embed_latency_ms: f64,
    pub search_latency_ms: f64,

    // Cryptographic signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignedReceipt>,
}

/// Signable subset of receipt (excludes signature field)
#[derive(Serialize)]
struct SignableReceipt<'a> {
    embedder_model_hash: &'a B3Hash,
    tokenizer_hash: &'a B3Hash,
    corpus_version_hash: &'a B3Hash,
    index_type: &'a str,
    index_params_hash: &'a B3Hash,
    index_seed: Option<u64>,
    query_text_hash: &'a B3Hash,
    query_embedding_hash: &'a B3Hash,
    top_k: &'a [(String, f32)],
    tenant_id: &'a str,
    request_id: &'a str,
    timestamp: &'a DateTime<Utc>,
}

impl RetrievalReceipt {
    /// Compute deterministic digest of receipt for signing
    pub fn compute_digest(&self) -> B3Hash {
        let signable = SignableReceipt {
            embedder_model_hash: &self.embedder_model_hash,
            tokenizer_hash: &self.tokenizer_hash,
            corpus_version_hash: &self.corpus_version_hash,
            index_type: &self.index_type,
            index_params_hash: &self.index_params_hash,
            index_seed: self.index_seed,
            query_text_hash: &self.query_text_hash,
            query_embedding_hash: &self.query_embedding_hash,
            top_k: &self.top_k,
            tenant_id: &self.tenant_id,
            request_id: &self.request_id,
            timestamp: &self.timestamp,
        };

        // Use canonical JSON for deterministic serialization
        let canonical = serde_json::to_vec(&signable)
            .expect("Receipt serialization should not fail");
        B3Hash::hash(&canonical)
    }

    /// Sign the receipt with Ed25519
    pub fn sign(&mut self, key_manager: &KeyManager) -> Result<()> {
        let digest = self.compute_digest();
        self.signature = Some(key_manager.sign_receipt(digest)?);
        Ok(())
    }

    /// Verify receipt signature
    pub fn verify(&self, key_manager: &KeyManager) -> Result<bool> {
        match &self.signature {
            Some(sig) => {
                let digest = self.compute_digest();
                key_manager.verify_receipt(&digest, sig)
            }
            None => Ok(false),
        }
    }
}
```

**Step 3: Run test**

Run: `cargo test -p adapteros-retrieval test_receipt_digest`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/adapteros-retrieval/src/receipt.rs
git commit -m "feat(retrieval): implement RetrievalReceipt with Ed25519 signing"
```

---

### Task 1.7: Implement eval metrics (Recall@K, nDCG, MRR)

**Files:**
- Create: `crates/adapteros-retrieval/src/eval.rs`

**Step 1: Write tests for metrics**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recall_at_k() {
        let relevant = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let retrieved = vec!["a".to_string(), "d".to_string(), "b".to_string()];

        assert!((recall_at_k(&relevant, &retrieved, 1) - 1.0/3.0).abs() < 1e-6);
        assert!((recall_at_k(&relevant, &retrieved, 2) - 1.0/3.0).abs() < 1e-6);
        assert!((recall_at_k(&relevant, &retrieved, 3) - 2.0/3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mrr() {
        // First relevant at position 1
        let relevant1 = vec!["a".to_string()];
        let retrieved1 = vec!["a".to_string(), "b".to_string()];
        assert!((mrr(&relevant1, &retrieved1) - 1.0).abs() < 1e-6);

        // First relevant at position 2
        let relevant2 = vec!["b".to_string()];
        let retrieved2 = vec!["a".to_string(), "b".to_string()];
        assert!((mrr(&relevant2, &retrieved2) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_ndcg() {
        let relevant = vec!["a".to_string(), "b".to_string()];
        let retrieved = vec!["a".to_string(), "c".to_string(), "b".to_string()];

        // DCG = 1/log2(2) + 0/log2(3) + 1/log2(4) = 1.0 + 0.5 = 1.5
        // IDCG = 1/log2(2) + 1/log2(3) = 1.0 + 0.631 = 1.631
        let ndcg = ndcg_at_k(&relevant, &retrieved, 3);
        assert!(ndcg > 0.9 && ndcg < 1.0);
    }
}
```

**Step 2: Implement metrics**

```rust
//! Evaluation metrics for retrieval quality

/// Compute Recall@K
///
/// Measures what fraction of relevant documents were retrieved in top-k
pub fn recall_at_k(relevant: &[String], retrieved: &[String], k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }

    let top_k: std::collections::HashSet<_> = retrieved.iter().take(k).collect();
    let found = relevant.iter().filter(|r| top_k.contains(r)).count();

    found as f64 / relevant.len() as f64
}

/// Compute Mean Reciprocal Rank (MRR)
///
/// Returns 1/rank of the first relevant document
pub fn mrr(relevant: &[String], retrieved: &[String]) -> f64 {
    let relevant_set: std::collections::HashSet<_> = relevant.iter().collect();

    for (i, doc) in retrieved.iter().enumerate() {
        if relevant_set.contains(doc) {
            return 1.0 / (i + 1) as f64;
        }
    }

    0.0
}

/// Compute nDCG@K (Normalized Discounted Cumulative Gain)
///
/// Measures ranking quality with position-weighted relevance
pub fn ndcg_at_k(relevant: &[String], retrieved: &[String], k: usize) -> f64 {
    let relevant_set: std::collections::HashSet<_> = relevant.iter().collect();

    // Compute DCG
    let dcg: f64 = retrieved
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, doc)| {
            let rel = if relevant_set.contains(doc) { 1.0 } else { 0.0 };
            rel / (i as f64 + 2.0).log2()
        })
        .sum();

    // Compute ideal DCG (all relevant docs at top)
    let ideal_k = k.min(relevant.len());
    let idcg: f64 = (0..ideal_k)
        .map(|i| 1.0 / (i as f64 + 2.0).log2())
        .sum();

    if idcg < 1e-9 {
        return 0.0;
    }

    dcg / idcg
}

/// Evaluation query with ground truth
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvalQuery {
    /// Query identifier
    pub query_id: String,
    /// Query text
    pub query_text: String,
    /// Relevant chunk IDs (ground truth)
    pub relevant_chunk_ids: Vec<String>,
    /// Hard negatives for training
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_negatives: Option<Vec<String>>,
    /// Source of query
    pub source: QuerySource,
}

/// Query source for tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum QuerySource {
    /// Generated from documentation
    Generated { from_doc: String },
    /// Manually curated
    Manual { annotator: String },
}

/// Compute all metrics for a set of queries
pub struct EvalResults {
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub recall_at_20: f64,
    pub ndcg_at_10: f64,
    pub mrr_at_10: f64,
    pub num_queries: usize,
}

impl EvalResults {
    /// Compute metrics from query results
    pub fn compute(
        queries: &[EvalQuery],
        results: &[Vec<String>], // Retrieved chunk IDs per query
    ) -> Self {
        let n = queries.len();
        if n == 0 {
            return Self {
                recall_at_5: 0.0,
                recall_at_10: 0.0,
                recall_at_20: 0.0,
                ndcg_at_10: 0.0,
                mrr_at_10: 0.0,
                num_queries: 0,
            };
        }

        let mut sum_recall_5 = 0.0;
        let mut sum_recall_10 = 0.0;
        let mut sum_recall_20 = 0.0;
        let mut sum_ndcg = 0.0;
        let mut sum_mrr = 0.0;

        for (query, retrieved) in queries.iter().zip(results) {
            sum_recall_5 += recall_at_k(&query.relevant_chunk_ids, retrieved, 5);
            sum_recall_10 += recall_at_k(&query.relevant_chunk_ids, retrieved, 10);
            sum_recall_20 += recall_at_k(&query.relevant_chunk_ids, retrieved, 20);
            sum_ndcg += ndcg_at_k(&query.relevant_chunk_ids, retrieved, 10);
            sum_mrr += mrr(&query.relevant_chunk_ids, retrieved);
        }

        Self {
            recall_at_5: sum_recall_5 / n as f64,
            recall_at_10: sum_recall_10 / n as f64,
            recall_at_20: sum_recall_20 / n as f64,
            ndcg_at_10: sum_ndcg / n as f64,
            mrr_at_10: sum_mrr / n as f64,
            num_queries: n,
        }
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p adapteros-retrieval eval::tests`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/adapteros-retrieval/src/eval.rs
git commit -m "feat(retrieval): implement eval metrics (Recall@K, nDCG, MRR)"
```

---

## Phase 2: Benchmark Harness

### Task 2.1: Implement BenchmarkHarness and BenchmarkReport

**Files:**
- Create: `crates/adapteros-retrieval/src/benchmark.rs`

**Step 1: Write test for benchmark report**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_report_serialization() {
        let report = BenchmarkReport {
            report_id: "test".to_string(),
            timestamp: Utc::now(),
            model_hash: B3Hash::hash(b"model"),
            model_name: "test-model".to_string(),
            is_finetuned: false,
            lora_adapter_hash: None,
            corpus_version_hash: B3Hash::hash(b"corpus"),
            num_chunks: 100,
            recall_at_k: [(5, 0.8), (10, 0.9)].into_iter().collect(),
            ndcg_at_10: 0.85,
            mrr_at_10: 0.75,
            embed_latency_p50_ms: 10.0,
            embed_latency_p99_ms: 25.0,
            throughput_per_sec: [(1, 100.0), (8, 500.0)].into_iter().collect(),
            memory_rss_mb: 512.0,
            index_build_time_ms: 1000.0,
            index_size_bytes: 1024 * 1024,
            determinism_pass: true,
            determinism_runs: 100,
            determinism_failures: vec![],
            receipts: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: BenchmarkReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model_name, "test-model");
    }
}
```

**Step 2: Implement types**

```rust
//! Benchmark harness for embedding evaluation

use crate::eval::{EvalQuery, EvalResults};
use crate::index::SearchResult;
use crate::receipt::RetrievalReceipt;
use adapteros_core::B3Hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Evaluation queries with ground truth
    pub eval_queries: Vec<EvalQuery>,
    /// K values for Recall@K
    pub k_values: Vec<usize>,
    /// Batch sizes for throughput testing
    pub batch_sizes: Vec<usize>,
    /// Number of determinism verification runs
    pub num_determinism_runs: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            eval_queries: vec![],
            k_values: vec![5, 10, 20],
            batch_sizes: vec![1, 8, 32],
            num_determinism_runs: 100,
        }
    }
}

/// Full benchmark report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// Report identifier
    pub report_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,

    // Model info
    pub model_hash: B3Hash,
    pub model_name: String,
    pub is_finetuned: bool,
    pub lora_adapter_hash: Option<B3Hash>,

    // Corpus info
    pub corpus_version_hash: B3Hash,
    pub num_chunks: usize,

    // Retrieval metrics
    pub recall_at_k: HashMap<usize, f64>,
    pub ndcg_at_10: f64,
    pub mrr_at_10: f64,

    // System metrics
    pub embed_latency_p50_ms: f64,
    pub embed_latency_p99_ms: f64,
    pub throughput_per_sec: HashMap<usize, f64>,
    pub memory_rss_mb: f64,
    pub index_build_time_ms: f64,
    pub index_size_bytes: u64,

    // Determinism verification
    pub determinism_pass: bool,
    pub determinism_runs: usize,
    pub determinism_failures: Vec<String>,

    // All receipts
    pub receipts: Vec<RetrievalReceipt>,
}

/// Determinism verification result
#[derive(Debug)]
pub struct DeterminismReport {
    pub total_runs: usize,
    pub total_queries: usize,
    pub passed: bool,
    pub failures: Vec<String>,
}

/// Benchmark harness
pub struct BenchmarkHarness {
    config: BenchmarkConfig,
}

impl BenchmarkHarness {
    /// Create new benchmark harness
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Get config
    pub fn config(&self) -> &BenchmarkConfig {
        &self.config
    }

    /// Verify determinism across multiple runs
    pub fn verify_determinism(
        &self,
        results_by_run: &[Vec<Vec<SearchResult>>],
    ) -> DeterminismReport {
        let num_runs = results_by_run.len();
        if num_runs < 2 {
            return DeterminismReport {
                total_runs: num_runs,
                total_queries: 0,
                passed: true,
                failures: vec![],
            };
        }

        let num_queries = results_by_run[0].len();
        let mut failures = vec![];

        // Compare all runs to first run
        let baseline = &results_by_run[0];
        for (run_idx, run) in results_by_run.iter().enumerate().skip(1) {
            for (query_idx, (base, current)) in baseline.iter().zip(run).enumerate() {
                if !Self::results_match(base, current) {
                    failures.push(format!(
                        "Query {} diverged on run {} vs baseline",
                        query_idx, run_idx
                    ));
                }
            }
        }

        DeterminismReport {
            total_runs: num_runs,
            total_queries: num_queries,
            passed: failures.is_empty(),
            failures,
        }
    }

    /// Check if two result sets match exactly
    fn results_match(a: &[SearchResult], b: &[SearchResult]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (ra, rb) in a.iter().zip(b) {
            if ra.chunk_id != rb.chunk_id {
                return false;
            }
            if (ra.score - rb.score).abs() > 1e-6 {
                return false;
            }
        }
        true
    }

    /// Compute percentile from sorted latencies
    pub fn percentile(sorted: &[f64], p: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}
```

**Step 3: Run test**

Run: `cargo test -p adapteros-retrieval benchmark::tests`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/adapteros-retrieval/src/benchmark.rs
git commit -m "feat(retrieval): implement BenchmarkHarness and BenchmarkReport"
```

---

## Phase 3: CLI Integration

### Task 3.1: Create embed subcommand structure

**Files:**
- Create: `crates/adapteros-cli/src/commands/embed.rs`
- Modify: `crates/adapteros-cli/src/commands/mod.rs`
- Modify: `crates/adapteros-cli/src/main.rs`

**Step 1: Create embed.rs with subcommands**

```rust
//! Embedding benchmark CLI commands

use adapteros_core::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Embedding operations: corpus, index, search, benchmark, train
#[derive(Args, Debug)]
pub struct EmbedArgs {
    #[command(subcommand)]
    pub command: EmbedCommand,
}

#[derive(Subcommand, Debug)]
pub enum EmbedCommand {
    /// Build corpus from docs and code
    Corpus(CorpusArgs),
    /// Build search index from corpus
    Index(IndexArgs),
    /// Search for similar chunks
    Search(SearchArgs),
    /// Run benchmark evaluation
    Bench(BenchArgs),
    /// Train embedding LoRA adapter
    Train(TrainArgs),
    /// Compare baseline vs fine-tuned
    Compare(CompareArgs),
}

#[derive(Args, Debug)]
pub struct CorpusArgs {
    /// Directory containing documentation
    #[arg(long)]
    pub docs_dir: Option<PathBuf>,
    /// Directory containing code
    #[arg(long)]
    pub code_dir: Option<PathBuf>,
    /// Output corpus JSON file
    #[arg(long, default_value = "corpus.json")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct IndexArgs {
    /// Input corpus JSON file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Output index directory
    #[arg(long, default_value = "./index")]
    pub output: PathBuf,
    /// Index type: flat or hnsw
    #[arg(long, default_value = "flat")]
    pub index_type: String,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Query text
    pub query: String,
    /// Index directory
    #[arg(long)]
    pub index: PathBuf,
    /// Number of results
    #[arg(long, default_value = "10")]
    pub top_k: usize,
}

#[derive(Args, Debug)]
pub struct BenchArgs {
    /// Corpus JSON file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Evaluation queries JSON file
    #[arg(long)]
    pub queries: PathBuf,
    /// Output report JSON file
    #[arg(long, default_value = "report.json")]
    pub output: PathBuf,
    /// LoRA adapter directory (optional)
    #[arg(long)]
    pub adapter: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct TrainArgs {
    /// Corpus JSON file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Training pairs JSONL file
    #[arg(long)]
    pub pairs: PathBuf,
    /// Output adapter directory
    #[arg(long, default_value = "./adapter")]
    pub output: PathBuf,
}

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// Baseline report JSON
    #[arg(long)]
    pub baseline: PathBuf,
    /// Fine-tuned report JSON
    #[arg(long)]
    pub finetuned: PathBuf,
}

impl EmbedArgs {
    pub async fn execute(&self) -> Result<()> {
        match &self.command {
            EmbedCommand::Corpus(args) => run_corpus(args).await,
            EmbedCommand::Index(args) => run_index(args).await,
            EmbedCommand::Search(args) => run_search(args).await,
            EmbedCommand::Bench(args) => run_bench(args).await,
            EmbedCommand::Train(args) => run_train(args).await,
            EmbedCommand::Compare(args) => run_compare(args).await,
        }
    }
}

async fn run_corpus(args: &CorpusArgs) -> Result<()> {
    println!("Building corpus...");
    println!("  Docs dir: {:?}", args.docs_dir);
    println!("  Code dir: {:?}", args.code_dir);
    println!("  Output: {}", args.output.display());
    // TODO: Implement corpus building
    Ok(())
}

async fn run_index(args: &IndexArgs) -> Result<()> {
    println!("Building index...");
    println!("  Corpus: {}", args.corpus.display());
    println!("  Type: {}", args.index_type);
    println!("  Output: {}", args.output.display());
    // TODO: Implement index building
    Ok(())
}

async fn run_search(args: &SearchArgs) -> Result<()> {
    println!("Searching...");
    println!("  Query: {}", args.query);
    println!("  Index: {}", args.index.display());
    println!("  Top-K: {}", args.top_k);
    // TODO: Implement search
    Ok(())
}

async fn run_bench(args: &BenchArgs) -> Result<()> {
    println!("Running benchmark...");
    println!("  Corpus: {}", args.corpus.display());
    println!("  Queries: {}", args.queries.display());
    println!("  Output: {}", args.output.display());
    // TODO: Implement benchmark
    Ok(())
}

async fn run_train(args: &TrainArgs) -> Result<()> {
    println!("Training adapter...");
    println!("  Corpus: {}", args.corpus.display());
    println!("  Pairs: {}", args.pairs.display());
    println!("  Output: {}", args.output.display());
    // TODO: Implement training
    Ok(())
}

async fn run_compare(args: &CompareArgs) -> Result<()> {
    println!("Comparing reports...");
    println!("  Baseline: {}", args.baseline.display());
    println!("  Fine-tuned: {}", args.finetuned.display());
    // TODO: Implement comparison
    Ok(())
}
```

**Step 2: Add to mod.rs**

Add `pub mod embed;` to the module list.

**Step 3: Wire up in main.rs**

Add `Embed(embed::EmbedArgs)` variant to the Commands enum and handle it.

**Step 4: Verify it builds and runs**

Run: `cargo build -p adapteros-cli`
Run: `./target/debug/aosctl embed --help`
Expected: Shows embed subcommands

**Step 5: Commit**

```bash
git add crates/adapteros-cli/src/commands/embed.rs
git add crates/adapteros-cli/src/commands/mod.rs
git add crates/adapteros-cli/src/main.rs
git commit -m "feat(cli): add embed subcommand structure"
```

---

### Task 3.2: Create benchmark script

**Files:**
- Create: `scripts/bench_embeddings.sh`

**Step 1: Create script**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Embedding Benchmark Runner
# Usage: ./scripts/bench_embeddings.sh [--train] [--output DIR]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/benchmark_results}"
TRAIN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --train)
            TRAIN=true
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

echo "=== Embedding Benchmark ==="
echo "Output directory: $OUTPUT_DIR"
echo ""

# Phase 1: Build corpus
echo "Phase 1: Building corpus..."
./aosctl embed corpus build \
    --docs-dir "$REPO_ROOT/docs" \
    --code-dir "$REPO_ROOT/crates" \
    --output "$OUTPUT_DIR/corpus.json"

# Phase 2: Baseline benchmark
echo ""
echo "Phase 2: Running baseline benchmark..."
./aosctl embed bench \
    --corpus "$OUTPUT_DIR/corpus.json" \
    --queries "$REPO_ROOT/eval/golden_queries.json" \
    --output "$OUTPUT_DIR/baseline_report.json"

# Phase 3: Fine-tune (optional)
if [[ "$TRAIN" == "true" ]]; then
    echo ""
    echo "Phase 3: Training LoRA adapter..."
    ./aosctl embed train \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --pairs "$OUTPUT_DIR/training_pairs.json" \
        --output "$OUTPUT_DIR/adapter/"

    echo ""
    echo "Phase 4: Running fine-tuned benchmark..."
    ./aosctl embed bench \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --queries "$REPO_ROOT/eval/golden_queries.json" \
        --adapter "$OUTPUT_DIR/adapter/" \
        --output "$OUTPUT_DIR/finetuned_report.json"

    echo ""
    echo "Phase 5: Comparing results..."
    ./aosctl embed compare \
        --baseline "$OUTPUT_DIR/baseline_report.json" \
        --finetuned "$OUTPUT_DIR/finetuned_report.json"
fi

echo ""
echo "=== Done ==="
echo "Results saved to: $OUTPUT_DIR/"
```

**Step 2: Make executable**

Run: `chmod +x scripts/bench_embeddings.sh`

**Step 3: Commit**

```bash
git add scripts/bench_embeddings.sh
git commit -m "feat(scripts): add bench_embeddings.sh runner"
```

---

## Phase 4-6: Training, UI, HNSW

These phases follow the same pattern. Key tasks:

- **4.1**: Implement contrastive loss in `adapteros-embeddings/src/training.rs`
- **4.2**: Implement LoRA adapter for embeddings in `adapteros-embeddings/src/lora.rs`
- **5.1**: Add embedding benchmark section to audit page
- **5.2**: Create EmbeddingTester component
- **6.1**: Implement HnswIndex with seeded build

Each follows TDD: write failing test → implement → verify pass → commit.

---

## Success Criteria

After completing all tasks:

| Criterion | Check |
|-----------|-------|
| `cargo test -p adapteros-embeddings` | All pass |
| `cargo test -p adapteros-retrieval` | All pass |
| `./aosctl embed --help` | Shows subcommands |
| Determinism: 100 runs same results | Verified |
| Receipt signing works | Ed25519 verified |
| UI compiles | `trunk build` succeeds |
