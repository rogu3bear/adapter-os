# Embedding Benchmark System Design

**Date**: 2026-01-23
**Status**: Approved

## Overview

Deterministic embedding benchmark system for AdapterOS. Three deliverables:
1. **Baseline**: MLX embedding model with full determinism tracking
2. **Fine-tune**: LoRA adapters trained on AdapterOS-specific pairs via MLX
3. **Proof**: Reproducible eval with retrieval receipts

## Decisions Summary

| Decision | Choice |
|----------|--------|
| Baseline model | Existing MLX embedding infrastructure |
| Eval dataset | Hybrid: docs-generated + manual golden queries |
| Chunking | Content-aware: token-based (docs) + semantic (code) |
| Vector index | Flat (ground truth) + HNSW (performance tier) |
| Fine-tuning | MLX native training with LoRA adapters |
| Receipt format | Full audit: seed lineage + tenant + Ed25519 signature |
| CLI integration | Both script and aosctl subcommands |
| UI | Audit page (history) + interactive testing panel |

---

## Crate Structure

### `crates/adapteros-embeddings/`

Core embedding logic with determinism guarantees.

```
Cargo.toml
src/
├── lib.rs           # Public API: embed(), embed_batch(), EmbeddingProvider
├── config.rs        # Config variables (AOS_EMBEDDING_*)
├── model.rs         # EmbeddingModel trait, MLX implementation
├── lora.rs          # EmbeddingLoraAdapter, adapter loading
├── training.rs      # ContrastiveLoss, MLX training loop, InfoNCE
└── determinism.rs   # ModelHash validation, seed isolation
```

**Dependencies**:
- `adapteros-core` (seeds, TypedSeed, SeedLineage)
- `adapteros-crypto` (B3Hash, BLAKE3)
- `adapteros-lora-mlx-ffi` (MLX backend)
- `adapteros-config` (config loading)

**Feature flags**:
```toml
[features]
default = ["mlx"]
mlx = ["adapteros-lora-mlx-ffi"]
training = []  # Enable training loop
```

### `crates/adapteros-retrieval/`

Retrieval, indexing, and benchmarking.

```
Cargo.toml
src/
├── lib.rs           # Public API: search(), benchmark(), RetrievalProvider
├── chunking.rs      # HybridChunker (delegates to existing impls)
├── corpus.rs        # CorpusBuilder, corpus_version_hash
├── index/
│   ├── mod.rs       # IndexBackend trait
│   ├── flat.rs      # FlatIndex (exact NN)
│   └── hnsw.rs      # HnswIndex (approximate NN)
├── receipt.rs       # RetrievalReceipt, signing
├── eval.rs          # recall_at_k(), ndcg(), mrr()
├── benchmark.rs     # BenchmarkHarness, BenchmarkReport
└── query_gen.rs     # Query generation from docs
```

**Dependencies**:
- `adapteros-embeddings` (embedding generation)
- `adapteros-crypto` (receipt signing)
- `adapteros-ingest-docs` (token chunker)
- `adapteros-lora-rag` (semantic chunker)
- `adapteros-telemetry` (event emission)

---

## Core Types

### EmbeddingModel Trait

```rust
#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    /// Embed a single text, returning normalized vector
    async fn embed(&self, text: &str) -> Result<Embedding>;

    /// Embed batch for throughput
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// Model identity for determinism
    fn model_hash(&self) -> &B3Hash;
    fn tokenizer_hash(&self) -> &B3Hash;
    fn embedding_dimension(&self) -> usize;
}

pub struct Embedding {
    pub vector: Vec<f32>,
    pub model_hash: B3Hash,
    pub input_hash: B3Hash,  // BLAKE3 of input text
}
```

### Chunk Types

```rust
pub struct Chunk {
    pub chunk_id: String,           // blake3(source_path + start_offset + length)
    pub source_path: String,
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub chunk_type: ChunkType,      // Code or Document
    pub content_hash: B3Hash,
}

pub enum ChunkType {
    Code { language: String, semantic_type: String },  // function, class, etc.
    Document { format: String },                        // markdown, text, etc.
}

pub struct ChunkingConfig {
    pub token_chunk_size: usize,    // Default: 512
    pub token_overlap: usize,       // Default: 128
    pub code_target_size: usize,    // Default: 1000 chars
    pub code_max_size: usize,       // Default: 2000 chars
}
```

### Corpus

```rust
pub struct Corpus {
    pub corpus_id: String,
    pub version_hash: B3Hash,       // Hash of all chunk hashes
    pub chunks: Vec<Chunk>,
    pub chunking_config: ChunkingConfig,
    pub created_at: DateTime<Utc>,
}

impl Corpus {
    /// Deterministic version hash
    pub fn compute_version_hash(chunks: &[Chunk]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();
        for chunk in chunks.iter().sorted_by_key(|c| &c.chunk_id) {
            hasher.update(chunk.content_hash.as_bytes());
        }
        B3Hash::from(hasher.finalize())
    }
}
```

### Index Backend

```rust
#[async_trait]
pub trait IndexBackend: Send + Sync {
    /// Build index from embeddings
    async fn build(&mut self, embeddings: &[(String, Embedding)]) -> Result<IndexMetadata>;

    /// Search for top-k nearest neighbors
    async fn search(&self, query: &Embedding, top_k: usize) -> Result<Vec<SearchResult>>;

    /// Index identity for receipts
    fn index_metadata(&self) -> &IndexMetadata;
}

pub struct IndexMetadata {
    pub index_type: String,         // "flat" or "hnsw"
    pub params_hash: B3Hash,        // Hash of build params
    pub build_seed: Option<u64>,    // For HNSW reproducibility
    pub num_vectors: usize,
    pub dimension: usize,
}

pub struct SearchResult {
    pub chunk_id: String,
    pub score: f32,
    pub rank: usize,
}
```

### Retrieval Receipt

```rust
#[derive(Serialize, Deserialize)]
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

    // Results (deterministic order)
    pub top_k: Vec<(String, f32)>,  // (chunk_id, score)

    // Seed lineage (from adapteros-core)
    pub seed_lineage: SeedLineage,

    // Tenant context
    pub tenant_id: String,
    pub request_id: String,
    pub timestamp: DateTime<Utc>,

    // Metrics snapshot
    pub embed_latency_ms: f64,
    pub search_latency_ms: f64,

    // Cryptographic signature
    pub signature: Option<SignedReceipt>,
}

impl RetrievalReceipt {
    pub fn compute_digest(&self) -> B3Hash {
        // Canonical JSON serialization, then BLAKE3
        let canonical = serde_json::to_vec(&self.to_signable()).unwrap();
        B3Hash::from(blake3::hash(&canonical))
    }

    pub fn sign(&mut self, key_manager: &KeyManager) -> Result<()> {
        let digest = self.compute_digest();
        self.signature = Some(key_manager.sign_receipt(digest)?);
        Ok(())
    }
}
```

### Benchmark Types

```rust
pub struct EvalQuery {
    pub query_id: String,
    pub query_text: String,
    pub relevant_chunk_ids: Vec<String>,    // Ground truth
    pub hard_negatives: Option<Vec<String>>, // For training
    pub source: QuerySource,
}

pub enum QuerySource {
    Generated { from_doc: String },
    Manual { annotator: String },
}

pub struct BenchmarkConfig {
    pub eval_queries: Vec<EvalQuery>,
    pub k_values: Vec<usize>,       // [5, 10, 20]
    pub batch_sizes: Vec<usize>,    // [1, 8, 32]
    pub num_determinism_runs: usize, // 100 for verification
}

pub struct BenchmarkReport {
    pub report_id: String,
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
    pub recall_at_k: HashMap<usize, f64>,   // k -> recall
    pub ndcg_at_10: f64,
    pub mrr_at_10: f64,

    // System metrics
    pub embed_latency_p50_ms: f64,
    pub embed_latency_p99_ms: f64,
    pub throughput_per_sec: HashMap<usize, f64>,  // batch_size -> throughput
    pub memory_rss_mb: f64,
    pub index_build_time_ms: f64,
    pub index_size_bytes: u64,

    // Determinism verification
    pub determinism_pass: bool,
    pub determinism_runs: usize,
    pub determinism_failures: Vec<String>,

    // All receipts for this run
    pub receipts: Vec<RetrievalReceipt>,
}
```

---

## Training (LoRA Fine-tune)

### Training Data Format

```rust
pub struct TrainingPair {
    pub query: String,
    pub positive: String,       // Relevant chunk content
    pub negatives: Vec<String>, // Hard negatives
}

pub struct TrainingConfig {
    pub lora_rank: usize,           // Default: 8
    pub lora_alpha: f32,            // Default: 16.0
    pub learning_rate: f64,         // Default: 1e-4
    pub batch_size: usize,          // Default: 32
    pub epochs: usize,              // Default: 3
    pub temperature: f32,           // InfoNCE temperature, default: 0.07
    pub in_batch_negatives: bool,   // Default: true
    pub early_stopping_patience: usize, // Default: 2
    pub eval_metric: String,        // "ndcg@10"
}
```

### Training Loop (MLX)

```rust
pub struct EmbeddingLoraTrainer {
    base_model: Arc<dyn EmbeddingModel>,
    lora_adapter: LoraAdapter,
    config: TrainingConfig,
    seed: TypedSeed,
}

impl EmbeddingLoraTrainer {
    pub async fn train(
        &mut self,
        train_pairs: &[TrainingPair],
        eval_queries: &[EvalQuery],
        corpus: &Corpus,
    ) -> Result<TrainingResult> {
        // 1. Initialize LoRA weights with deterministic seed
        // 2. For each epoch:
        //    a. Shuffle with seeded RNG
        //    b. For each batch:
        //       - Embed queries and positives
        //       - Compute InfoNCE loss with in-batch negatives
        //       - Backward pass, update LoRA weights
        //    c. Eval on held-out set
        //    d. Early stopping check on nDCG@10
        // 3. Return best checkpoint
    }
}

pub struct TrainingResult {
    pub final_loss: f64,
    pub best_ndcg_at_10: f64,
    pub epochs_trained: usize,
    pub lora_weights_hash: B3Hash,
    pub training_seed: TypedSeed,
}
```

---

## CLI Commands

### `aosctl embed`

```bash
# Build corpus from docs
aosctl embed corpus build --docs-dir ./docs --code-dir ./crates --output ./corpus.json

# Embed corpus (builds index)
aosctl embed index build --corpus ./corpus.json --output ./index/ --type flat

# Run single query
aosctl embed search "how does seed derivation work" --index ./index/ --top-k 10

# Run benchmark
aosctl embed bench --corpus ./corpus.json --queries ./eval_queries.json --output ./report.json

# Train LoRA adapter
aosctl embed train --corpus ./corpus.json --pairs ./training_pairs.json --output ./adapter/

# Compare baseline vs fine-tuned
aosctl embed compare --baseline ./report_baseline.json --finetuned ./report_finetuned.json
```

### Script: `scripts/bench_embeddings.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# One-shot benchmark runner
# Usage: ./scripts/bench_embeddings.sh [--train] [--output DIR]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/benchmark_results}"

# Phase 1: Build corpus
echo "Building corpus..."
./aosctl embed corpus build \
    --docs-dir "$REPO_ROOT/docs" \
    --code-dir "$REPO_ROOT/crates" \
    --output "$OUTPUT_DIR/corpus.json"

# Phase 2: Baseline benchmark
echo "Running baseline benchmark..."
./aosctl embed bench \
    --corpus "$OUTPUT_DIR/corpus.json" \
    --queries "$REPO_ROOT/eval/golden_queries.json" \
    --output "$OUTPUT_DIR/baseline_report.json"

# Phase 3: Fine-tune (optional)
if [[ "${1:-}" == "--train" ]]; then
    echo "Training LoRA adapter..."
    ./aosctl embed train \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --pairs "$OUTPUT_DIR/training_pairs.json" \
        --output "$OUTPUT_DIR/adapter/"

    echo "Running fine-tuned benchmark..."
    ./aosctl embed bench \
        --corpus "$OUTPUT_DIR/corpus.json" \
        --queries "$REPO_ROOT/eval/golden_queries.json" \
        --adapter "$OUTPUT_DIR/adapter/" \
        --output "$OUTPUT_DIR/finetuned_report.json"

    echo "Comparing results..."
    ./aosctl embed compare \
        --baseline "$OUTPUT_DIR/baseline_report.json" \
        --finetuned "$OUTPUT_DIR/finetuned_report.json"
fi

echo "Done. Results in $OUTPUT_DIR/"
```

---

## UI Components

### Audit Page Addition

Add "Embedding Benchmarks" section to existing Audit page (`src/pages/audit/`):

```rust
// src/pages/audit/embedding_benchmarks.rs

#[component]
pub fn EmbeddingBenchmarkHistory() -> impl IntoView {
    // List of benchmark runs with:
    // - Timestamp, model name, corpus version
    // - Key metrics: nDCG@10, Recall@10, determinism status
    // - Expand to see full report + receipts
    // - Download receipt JSON
}
```

### Interactive Testing Panel

New component, accessible from multiple places:

```rust
// src/components/embedding_tester.rs

#[component]
pub fn EmbeddingTester() -> impl IntoView {
    // Collapsible panel or modal with:
    // - Query input text box
    // - "Search" button
    // - Results list: chunk preview, score, source file
    // - Toggle to show/hide receipt details
    // - Copy receipt to clipboard
}
```

Placement:
- Floating action button on Audit page
- Optional sidebar in Chat page (for RAG debugging)

---

## Determinism Verification

### Requirements

1. **Same input → same embedding bytes**
   - Hash embedding vector, compare across runs

2. **Same query → same top-K results**
   - Compare (chunk_id, score) tuples exactly
   - Scores must match to float precision

3. **Reproducible from receipt**
   - Given a receipt, re-run query, get identical results

### Verification Process

```rust
pub async fn verify_determinism(
    model: &dyn EmbeddingModel,
    index: &dyn IndexBackend,
    queries: &[EvalQuery],
    num_runs: usize,
) -> DeterminismReport {
    let mut results_by_query: HashMap<String, Vec<Vec<SearchResult>>> = HashMap::new();

    for _ in 0..num_runs {
        for query in queries {
            let embedding = model.embed(&query.query_text).await?;
            let results = index.search(&embedding, 10).await?;
            results_by_query
                .entry(query.query_id.clone())
                .or_default()
                .push(results);
        }
    }

    // Check all runs match
    let mut failures = vec![];
    for (query_id, runs) in &results_by_query {
        let first = &runs[0];
        for (i, run) in runs.iter().enumerate().skip(1) {
            if !results_match(first, run) {
                failures.push(format!("Query {} diverged on run {}", query_id, i));
            }
        }
    }

    DeterminismReport {
        total_runs: num_runs,
        total_queries: queries.len(),
        passed: failures.is_empty(),
        failures,
    }
}
```

---

## Success Gates

| Gate | Threshold |
|------|-----------|
| Uplift | +5% nDCG@10 over baseline |
| Embed latency p50 | < 50ms (batch=1) |
| Determinism | 100/100 runs match |
| Reproducibility | Fresh checkout reproduces report |

---

## File Changes Summary

### New Files

```
crates/adapteros-embeddings/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── config.rs
    ├── model.rs
    ├── lora.rs
    ├── training.rs
    └── determinism.rs

crates/adapteros-retrieval/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── chunking.rs
    ├── corpus.rs
    ├── index/
    │   ├── mod.rs
    │   ├── flat.rs
    │   └── hnsw.rs
    ├── receipt.rs
    ├── eval.rs
    ├── benchmark.rs
    └── query_gen.rs

crates/adapteros-cli/src/commands/
└── embed.rs                    # New subcommand

crates/adapteros-ui/src/
├── pages/audit/
│   └── embedding_benchmarks.rs # New component
└── components/
    └── embedding_tester.rs     # New component

scripts/
└── bench_embeddings.sh

docs/
└── EMBEDDINGS_BENCHMARK.md
```

### Modified Files

```
Cargo.toml                      # Add workspace members
crates/adapteros-cli/Cargo.toml # Add dependencies
crates/adapteros-cli/src/commands/mod.rs  # Register embed command
crates/adapteros-cli/src/main.rs          # Wire up command
crates/adapteros-ui/src/pages/audit/mod.rs # Add benchmark section
crates/adapteros-ui/src/components/mod.rs  # Export new component
crates/adapteros-server-api/src/handlers/mod.rs # Add benchmark endpoints (optional)
```

---

## Implementation Order

1. **Phase 1: Core crates** (parallel)
   - `adapteros-embeddings` - model trait, MLX impl, config
   - `adapteros-retrieval` - chunking, corpus, flat index

2. **Phase 2: Benchmark harness**
   - Eval metrics (recall, nDCG, MRR)
   - Receipt generation and signing
   - Determinism verification

3. **Phase 3: CLI integration**
   - `aosctl embed` subcommands
   - `bench_embeddings.sh` script

4. **Phase 4: Training**
   - LoRA adapter for embeddings
   - Contrastive loss in MLX
   - Training loop with early stopping

5. **Phase 5: UI**
   - Audit page benchmark history
   - Interactive testing panel

6. **Phase 6: HNSW index**
   - Approximate search backend
   - Determinism verification for ANN
