# Training Pipeline

**Purpose:** End-to-end documentation for adapter training workflow in AdapterOS

**Last Updated:** 2025-11-22

---

## Overview

The training pipeline converts documents into trained LoRA adapters packaged as `.aos` archives.

For the flow diagram and detailed patterns, see:
- [ARCHITECTURE_PATTERNS.md#training-pipeline](ARCHITECTURE_PATTERNS.md#training-pipeline) - End-to-end flow diagram

---

## Pipeline Stages

### 1. Document Ingestion

**Source:** `crates/adapteros-ingest-docs`

```rust
use adapteros_ingest_docs::{DocumentIngestor, IngestOptions};

let opts = IngestOptions::default();
let ingestor = DocumentIngestor::new(opts, tokenizer);
let doc = ingestor.ingest_pdf_path(path)?;
```

Supported formats: PDF, plain text, markdown

### 2. Training Data Generation

**Source:** `crates/adapteros-orchestrator/src/training_dataset_integration.rs`

```rust
let examples = generate_training_data(&doc, &tokenizer, &config)?;
```

Strategies:
- **Identity:** Unsupervised (input == target)
- **QuestionAnswer:** Q&A pairs extracted from content
- **MaskedLM:** Masked language modeling

### 3. Dataset Creation

```rust
use adapteros_orchestrator::TrainingDatasetManager;

let manager = TrainingDatasetManager::new(db, path, tok);
let dataset = manager.create_dataset_from_documents(req).await?;
```

Properties:
- BLAKE3 content addressing
- Schema validation
- `validation_status` must be `'valid'` before training

### 4. Training

**Source:** `crates/adapteros-lora-worker/src/training/trainer.rs`

```rust
use adapteros_lora_worker::training::MicroLoRATrainer;

let trainer = MicroLoRATrainer::new(cfg)?;
let weights = trainer.train(examples, adapter_id).await?;
```

Configuration:
- `rank`: LoRA rank (default 16)
- `alpha`: LoRA alpha scaling (default 32)
- `epochs`: Training epochs
- `learning_rate`: Optimizer LR

Templates:
- `general-code`: rank=16, alpha=32 (multi-language)
- `framework-specific`: rank=12, alpha=24

### 5. Packaging

**Source:** `crates/adapteros-lora-worker/src/training/packager.rs`

```rust
use adapteros_lora_worker::training::AdapterPackager;

let packager = AdapterPackager::with_default_path();
let packaged = packager
    .package_aos_for_tenant("tenant_id", "adapter-id", &quantized, &config, "base-model")
    .await?;
```

Output: `.aos` archive with safetensors weights and JSON manifest (rank, alpha, training_backend, determinism, `gate_q15_denominator=32767`, `quantization="q15"`).

### 6. Registration

```rust
use adapteros_registry::Registry;

let registry = Registry::open("./registry.db")?;
registry.register_adapter(&adapter_id, &hash, "tier_1", rank, &["tenant_a"])?;
```

---

## Job Tracking

Training jobs progress through states:
- **Pending** - Queued for execution
- **Running** - Currently training (progress %, loss, tokens/sec)
- **Completed** - Successfully finished
- **Failed** - Error during training
- **Cancelled** - User-cancelled

**Database:** `training_jobs` table

### Backend, Determinism, and Packaging Surface
- Trainer records backend (`coreml`, `metal`, `mlx`, `cpu`) with a selection reason and GPU requirements (`require_gpu`, `max_gpu_memory_mb`).
- `TrainingService::run_training_job` calls `init_kernels()` before entering the loop, using plan bytes resolved from `AOS_MODEL_PATH`; when GPU is optional and init fails or assets are missing, it logs and runs on CPU.
- Determinism is HKDF-seeded; `training_seed` and `determinism_mode` are exposed on the job for audit.
- Extended metrics: `examples_processed`, `tokens_processed` (approx.), `throughput_examples_per_sec`, `gpu_utilization_pct`, `peak_gpu_memory_mb`, `training_time_ms`.
- Packaging returns `.aos` path/hash plus manifest summary (rank, base_model, per-layer hashes present) and `signature_status`; artifacts and job responses carry these fields.

### UI/Operator Surface
- Job list shows backend + determinism badges with status/progress.
- Job detail shows backend/determinism seed, GPU requirements, package path/hash, manifest summary, and signature status; metrics tab includes throughput/GPU fields when present.

---

## Validation Gates

1. **Dataset validation:** `validation_status = 'valid'` required
2. **BLAKE3 hashing:** Content-addressed datasets
3. **Schema validation:** JSON schema compliance
4. **Manifest signing:** Ed25519 signature after packaging

---

## AdapterOS Training Quick Start

1. **Enable a backend**  
   - macOS default: `cargo build --release` (CoreML + deterministic-only)  
   - CPU-only/CI: `cargo build --release --no-default-features --features deterministic-only`  
   - Real MLX: `cargo build --release --features "multi-backend,mlx"`
2. **Ingest documents** → `DocumentIngestor::new(opts, tokenizer).ingest_pdf_path(path)?`
3. **Create dataset** → `TrainingDatasetManager::new(db, path, tok).create_dataset_from_documents(req).await?`
4. **Launch training** → `TrainingService::start_training_job(adapter_name, config, template_id, repo_id, dataset_id, db, storage_root)`  
   (defaults package + register + create stack; set `post_actions` to opt out)
5. **Find the adapter** → `.aos` at `var/adapters/repo/<tenant>/<adapter_id>/<adapter_id>.aos` with manifest fields for `training_backend`, `determinism`, and `gate_q15_denominator=32767` automatically registered via `AdapterRegistrationBuilder`.

---

## References

- [ARCHITECTURE_PATTERNS.md#training-pipeline](ARCHITECTURE_PATTERNS.md#training-pipeline) - Flow diagram
- [GPU_TRAINING_INTEGRATION.md](GPU_TRAINING_INTEGRATION.md) - GPU training setup
- [DATASET_TRAINING_INTEGRATION.md](DATASET_TRAINING_INTEGRATION.md) - Dataset integration
- [training/BASE_ADAPTER.md](training/BASE_ADAPTER.md) - Base adapter training
- [USER_GUIDE_DATASETS.md](USER_GUIDE_DATASETS.md) - Dataset user guide
- [CLAUDE.md](../CLAUDE.md) - Developer quick reference

MLNavigator Inc 2025-12-08.
