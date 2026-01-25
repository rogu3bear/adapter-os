# Document-Based Training Datasets

This directory contains document-based datasets for TrainingDatasetManager, generated from adapterOS documentation using the `adapteros-ingest-docs` pipeline.

## Overview

Two datasets are provided to demonstrate different training strategies:

1. **Identity Dataset** (`adapteros_identity/`) - Memorization training
2. **Q&A Dataset** (`adapteros_qa/`) - Instruction tuning

## Dataset 1: adapterOS Identity Dataset

**Path:** `training/datasets/docs/adapteros_identity/`

**Strategy:** Identity (unsupervised memorization)

**Description:**
Identity mapping dataset where `input = target`. Trains adapters to reproduce adapterOS documentation verbatim, teaching core concepts, conventions, and patterns through memorization.

**Statistics:**
- **Examples:** 10 training examples
- **Source:** AGENTS.md (adapterOS Developer Guide)
- **Sections:** header, code_style, error_handling, logging, policy_packs, naming_conventions, hot_swap, lifecycle, deterministic_seeding, hkdf
- **Avg Sequence Length:** ~350 tokens

**Format:**
```json
{
  "input": "<documentation text>",
  "target": "<identical documentation text>",
  "metadata": {
    "source": "AGENTS.md",
    "strategy": "identity",
    "section": "error_handling",
    "chunk_index": "2"
  }
}
```

**Recommended Training Config:**
- Rank: 16
- Alpha: 32
- Max Seq Length: 512
- Strategy: `TrainingStrategy::Identity`

**Use Case:** Training adapters to memorize and recall adapterOS technical documentation, conventions, and code patterns.

---

## Dataset 2: adapterOS Q&A Dataset

**Path:** `training/datasets/docs/adapteros_qa/`

**Strategy:** QuestionAnswer (instruction tuning)

**Description:**
Question-answer pairs generated from AGENTS.md for instruction tuning. Teaches adapters to answer questions about adapterOS architecture, conventions, and best practices.

**Statistics:**
- **Examples:** 20 Q&A pairs
- **Source:** AGENTS.md (adapterOS Developer Guide)
- **Topics:** overview, error_handling, logging, policies, naming, hot_swap, lifecycle, determinism, hkdf, rbac, aos_format, memory_management, barrier_telemetry, heartbeat, training_pipeline, training_strategies, migrations, pinning, ttl, concurrency
- **Avg Question Length:** ~50 tokens
- **Avg Answer Length:** ~200 tokens

**Format:**
```json
{
  "input": "What is adapterOS?",
  "target": "adapterOS is a technical platform for K-sparse LoRA routing...",
  "metadata": {
    "source": "AGENTS.md",
    "strategy": "qa",
    "topic": "overview"
  }
}
```

**Recommended Training Config:**
- Rank: 16
- Alpha: 32
- Max Seq Length: 512
- Strategy: `TrainingStrategy::QuestionAnswer`

**Use Case:** Training adapters to answer technical questions about adapterOS features, architecture, and usage patterns.

**Question Types:**
- What is X? (definitions)
- How does X work? (mechanisms)
- What are the X in Y? (enumerations)
- Best practices for X? (conventions)
- Technical details about X? (deep dives)

---

## Usage with TrainingDatasetManager

### 1. Create Dataset from Documents

```rust
use adapteros_orchestrator::training_dataset_integration::TrainingDatasetManager;
use adapteros_ingest_docs::{DocumentIngestor, TrainingGenConfig, TrainingStrategy};
use std::sync::Arc;

// Load tokenizer
let tokenizer = adapteros_ingest_docs::load_tokenizer(
    &std::path::Path::new("models/test-model/tokenizer.json")
)?;

// Initialize ingestor
let ingestor = DocumentIngestor::new(
    adapteros_ingest_docs::default_ingest_options(),
    Some(tokenizer.clone())
);

// Ingest AGENTS.md
let document = ingestor.ingest_markdown_path("AGENTS.md")?;

// Generate training data (identity strategy)
let config = TrainingGenConfig {
    strategy: TrainingStrategy::Identity,
    max_seq_length: 512,
    add_special_tokens: true,
};
let training_data = adapteros_ingest_docs::generate_training_data(
    &document,
    &tokenizer,
    &config
)?;

// Or generate Q&A pairs
let qa_config = TrainingGenConfig {
    strategy: TrainingStrategy::QuestionAnswer,
    max_seq_length: 512,
    add_special_tokens: true,
};
let qa_data = adapteros_ingest_docs::generate_training_data(
    &document,
    &tokenizer,
    &qa_config
)?;
```

### 2. Register Dataset with TrainingDatasetManager

```rust
use adapteros_orchestrator::training_dataset_integration::{
    TrainingDatasetManager, CreateDatasetRequest
};

let manager = TrainingDatasetManager::new(
    db.clone(),
    "var/datasets".into(),
    tokenizer
).await?;

let request = CreateDatasetRequest {
    name: "adapteros-identity-docs-v1".to_string(),
    description: Some("Identity mapping from AGENTS.md".to_string()),
    tenant_id: "default".to_string(),
    domain: Some("engineering/documentation".to_string()),
    tags: vec!["documentation".into(), "identity".into()],
    source_type: "markdown".to_string(),
    validation_config: None,
};

let dataset_id = manager.create_dataset_from_documents(
    request,
    vec![document]
).await?;
```

### 3. Train Adapter

```rust
use adapteros_lora_worker::training::MicroLoRATrainer;

let trainer = MicroLoRATrainer::new(MicroLoRAConfig {
    rank: 16,
    alpha: 32.0,
    learning_rate: 1e-4,
    num_epochs: 3,
    batch_size: 4,
})?;

let adapter_weights = trainer.train(
    training_data.examples,
    "adapteros-docs-adapter"
).await?;
```

---

## File Structure

```
training/datasets/docs/
├── README.md                           # This file
├── adapteros_identity/
│   ├── manifest.json                   # Dataset metadata
│   └── adapteros-identity.jsonl        # 10 identity examples
└── adapteros_qa/
    ├── manifest.json                   # Dataset metadata
    └── adapteros-qa.jsonl              # 20 Q&A pairs
```

---

## Manifest Schema

Each dataset includes a `manifest.json` with:

```json
{
  "dataset_id": "unique-id",
  "name": "Human-readable name",
  "description": "Dataset purpose and contents",
  "version": "1.0.0",
  "created_at": "2025-01-18",
  "strategy": "identity | question_answer",
  "source_documents": [...],
  "statistics": {
    "total_examples": 10,
    "avg_sequence_length": 350
  },
  "training_config": {
    "recommended_rank": 16,
    "recommended_alpha": 32,
    "max_seq_length": 512,
    "strategy": "Identity"
  },
  "metadata": {
    "purpose": "...",
    "domain": "...",
    "use_case": "...",
    "quality": "high",
    "validation_status": "valid"
  }
}
```

---

## Validation

All datasets are pre-validated with `validation_status = "valid"` in manifests. To verify format:

```bash
# Check JSONL format
python3 -m json.tool < adapteros_identity/adapteros-identity.jsonl
python3 -m json.tool < adapteros_qa/adapteros-qa.jsonl

# Count examples
wc -l adapteros_identity/*.jsonl adapteros_qa/*.jsonl
```

---

## Integration with Document Ingestion Pipeline

These datasets demonstrate the training pipeline used by the control plane:

1. **Ingest** → `DocumentIngestor::{ingest_markdown_path, ingest_pdf_path}()`
2. **Chunk + Save** → document chunks saved as JSONL rows `{ "text": "..." }`
3. **Dataset** → `TrainingDatasetManager::create_dataset_from_documents()`
4. **Train** → orchestrator `TrainingPipeline` phases (dataset_build → preprocess → split → training_loop → validation_early_stopping)
5. **Package** → `package_and_register_adapter()` → `.aos`

---

## References

- **Document Ingestion:** `crates/adapteros-ingest-docs/src/lib.rs`
- **Training Generation:** `crates/adapteros-ingest-docs/src/training_gen.rs`
- **Dataset Manager:** `crates/adapteros-orchestrator/src/training_dataset_integration.rs`
- **Trainer:** `crates/adapteros-lora-worker/training/`
- **Developer Guide:** `AGENTS.md` (source document)

---

**Created:** 2025-01-18
**Maintained by:** adapterOS Training Pipeline
**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
