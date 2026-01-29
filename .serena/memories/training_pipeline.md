# AdapterOS Training Pipeline

## Overview

The training pipeline lives in `crates/adapteros-lora-worker/src/training/` and provides a complete LoRA (Low-Rank Adaptation) training infrastructure for Apple Silicon with deterministic guarantees.

## Key Entry Points

### Start Training

```rust
use adapteros_lora_worker::training::{
    MicroLoRATrainer, TrainingConfig, TrainingResult, TrainingExample
};

// Configure training
let config = TrainingConfig {
    rank: 4,                    // LoRA rank (bottleneck dimension)
    alpha: 16.0,                // LoRA scaling factor
    learning_rate: 1e-4,
    batch_size: 8,
    epochs: 3,
    hidden_dim: 768,            // Must match base model
    vocab_size: 32000,
    validation_split: 0.2,      // 20% for validation
    use_gpu_backward: true,     // GPU-accelerated gradients via MLX
    ..Default::default()
}
.with_base_model("/var/models/Llama-3.2-3B-Instruct-4bit");

// Create trainer and run
let mut trainer = MicroLoRATrainer::new(config)?;
let result: TrainingResult = trainer.train(&examples).await?;
```

### Resume from Checkpoint

```rust
use adapteros_lora_worker::training::{CheckpointManager, TrainingCheckpoint};

let manager = CheckpointManager::new(
    "./checkpoints",
    5,                          // save every 5 epochs
    3,                          // keep max 3 checkpoints
    "my-adapter".to_string()
);

// Check for existing checkpoint
if manager.has_checkpoint().await {
    let checkpoint = manager.load_latest().await?;
    // Resume training from checkpoint.epoch
}
```

## Key Types

### TrainingConfig (trainer/types.rs:386-523)

Main configuration struct with these important fields:

| Field | Type | Purpose |
|-------|------|---------|
| `rank` | `usize` | LoRA rank (bottleneck size, typically 4-16) |
| `alpha` | `f32` | Scaling factor (alpha/rank applied to output) |
| `learning_rate` | `f32` | Learning rate for optimizer |
| `batch_size` | `usize` | Examples per batch |
| `epochs` | `usize` | Number of training epochs |
| `hidden_dim` | `usize` | Hidden state dimension (must match model) |
| `vocab_size` | `usize` | Vocabulary size for CE loss |
| `validation_split` | `f32` | Fraction for validation (0.0-0.5) |
| `use_gpu_backward` | `bool` | GPU gradients via MLX (default: true) |
| `base_model_path` | `Option<PathBuf>` | REQUIRED for GPU training |
| `targets` | `Vec<String>` | Target modules (default: ["q_proj", "v_proj"]) |
| `multi_module_training` | `bool` | Train separate weights per module |
| `early_stopping` | `Option<bool>` | Enable early stopping |
| `patience` | `Option<u32>` | Epochs without improvement |

### LoRAWeights (trainer/types.rs:998-1162)

Trained weight matrices:

```rust
pub struct LoRAWeights {
    // Multi-module: per-target weights
    pub modules: HashMap<String, ModuleWeights>,
    
    // Legacy single-module
    pub lora_a: Vec<Vec<f32>>,  // [rank, hidden_dim]
    pub lora_b: Vec<Vec<f32>>,  // [hidden_dim, rank]
    
    pub moe_config: Option<MoETrainingConfig>,
    pub precomputed_delta: Option<Vec<Vec<f32>>>,
}

// Multi-module creation
LoRAWeights::new_multi_module(rank, hidden_dim, &["q_proj", "v_proj"]);
```

### TrainingResult (trainer/types.rs:717-817)

Returned after training:

```rust
pub struct TrainingResult {
    pub adapter_id: String,
    pub final_loss: f32,
    pub training_time_us: u64,
    pub weights: LoRAWeights,
    pub cancelled: bool,
    pub loss_curve: Vec<f32>,
    pub validation_loss_curve: Vec<f32>,
    pub train_perplexity_curve: Vec<f32>,
    pub best_validation: Option<(f32, u32)>,
    pub backend: Option<String>,
    pub using_gpu: bool,
    // ...
}
```

## Dataset Handling

### Build Dataset from JSONL (builder.rs)

```rust
use adapteros_lora_worker::training::{DatasetBuilder, DatasetSource};

let builder = DatasetBuilder::new(tokenizer_path, output_dir)
    .with_name("my-dataset");

let result = builder.build(&DatasetSource::Filesystem(
    PathBuf::from("./data.jsonl")
))?;
// Outputs: examples.jsonl + DatasetManifest.json
```

### JSONL Format (PLAN_4 constraints)

```json
{"prompt": "What is Rust?", "completion": "A systems programming language.", "schema": "supervised"}
```

Required fields: `prompt`/`input`, `completion`/`target`, `schema` (must be "supervised" or "raw_continuation_v1")

### Train/Validation Split (dataset.rs:357-426)

```rust
use adapteros_lora_worker::training::split_examples_for_validation;

let (train, validation, summary) = split_examples_for_validation(
    &examples,
    0.2,        // validation ratio
    42          // seed for determinism
);
// summary.split_hash_b3 is deterministic for same data+seed
```

## Loss Functions

### Primary: Cross-Entropy (loss.rs)

```rust
use adapteros_lora_worker::training::loss::{training_loss_spec, LossSpec, LossKind};

let spec = training_loss_spec(LOSS_IGNORE_INDEX);
// spec.kind = LossKind::CrossEntropy
// spec.logits_source = LossLogitsSource::HiddenPlusLoraProjection
```

### Embedding Training (embedding_loss.rs)

```rust
use adapteros_lora_worker::training::{
    triplet_loss, info_nce_loss, contrastive_loss, cosine_similarity
};

// Triplet loss for embeddings
let loss = triplet_loss(&anchor, &positive, &negative, 0.5);

// InfoNCE for in-batch negative sampling
let loss = info_nce_loss(&queries, &positives, 0.07);
```

## Checkpointing (checkpoint.rs)

### TrainingCheckpoint

```rust
pub struct TrainingCheckpoint {
    pub epoch: u32,
    pub step: u32,
    pub loss: f32,
    pub learning_rate: f32,
    pub config: TrainingConfig,
    pub weights: LoRAWeights,
    pub best_loss: f32,
    pub epochs_without_improvement: u32,
    pub timestamp: String,
    // ...
}

// Save atomically (temp file + rename)
checkpoint.save("./checkpoints/latest.ckpt").await?;

// Load with validation
let loaded = TrainingCheckpoint::load("./checkpoints/latest.ckpt").await?;
```

### CheckpointManager

- Automatic cleanup of old checkpoints
- `save_frequency`: save every N epochs
- `max_checkpoints`: keep only N most recent
- File pattern: `{adapter_id}_epoch_{N:04}.ckpt`, `{adapter_id}_latest.ckpt`

## Quantization (quantizer.rs)

```rust
use adapteros_lora_worker::training::{LoRAQuantizer, QuantizedLoRAWeights, LORA_Q15_DENOM};

// Quantize f32 -> i16 Q15
let quantized: QuantizedLoRAWeights = LoRAQuantizer::quantize_to_q15(&weights);

// Dequantize back for inference
let dequantized: LoRAWeights = LoRAQuantizer::dequantize_from_q15(&quantized);

// Q15 constants
// LORA_Q15_DENOM = 32767.0
// Range: [-1.0, 1.0] -> [-32767, 32767]
```

## Packaging (packager/)

### Package as .aos Archive

```rust
use adapteros_lora_worker::training::{AdapterPackager, QuantizedLoRAWeights};

let packager = AdapterPackager::new("./var/adapters/repo");

let packaged = packager.package_aos_with_metadata(
    "tenant-1",
    "my-adapter",
    &quantized_weights,
    &config,
    "Llama-3.2-3B-Instruct-4bit",
    metadata
).await?;

// packaged.weights_path -> content-addressed path
// packaged.hash_b3 -> BLAKE3 hash
// Creates draft ref: refs/{adapter}/tenant-1/draft
```

### AdapterManifest (packager/manifest.rs)

Contains:
- `version`, `rank`, `base_model`
- `training_config`
- `weights_hash` (BLAKE3)
- `per_layer_hashes`
- `coreml_placement` spec
- `integrity_hash` (sealed provenance)
- Metadata: scope, branch, commit, etc.

## Learning Rate Schedules (learning_rate_schedule.rs)

```rust
use adapteros_lora_worker::training::{LRScheduler, LRSchedulerConfig, LRScheduleType};

// Constant
let config = LRSchedulerConfig::constant(0.001);

// Cosine with warmup
let config = LRSchedulerConfig::cosine(0.01, 0.001, 1000)
    .with_warmup(100);

let mut scheduler = LRScheduler::new(config);
let lr = scheduler.get_lr();
scheduler.step();
```

## Early Stopping (early_stopping.rs)

```rust
use adapteros_lora_worker::training::{EarlyStopping, EarlyStoppingConfig};

let config = EarlyStoppingConfig::with_patience(5).with_min_delta(0.001);
let mut early_stop = EarlyStopping::new(config);

for epoch in 0..100 {
    let loss = train_epoch();
    early_stop.check(epoch, loss);
    
    if early_stop.should_stop() {
        println!("Stopping at epoch {} (best: {})", 
            early_stop.best_epoch(), early_stop.best_loss());
        break;
    }
}
```

## Backend Selection

Trainer auto-selects backend in this order:
1. User preference (`config.preferred_backend`)
2. CoreML (ANE) if available
3. MLX if available
4. Metal GPU
5. CPU (if `require_gpu: false`)

```rust
// Force specific backend
let config = config.with_backend(TrainingBackend::Mlx);

// Require GPU (error if unavailable)
let config = config.with_gpu_required();
```

## Key Patterns

### Deterministic Seeding

```rust
// Explicit seed
let config = TrainingConfig {
    determinism: Some(DeterminismConfig {
        seed: Some(42),
        dataset_version_id: Some("v1.0".into()),
        ..Default::default()
    }),
    ..Default::default()
};

// Trainer derives reproducible seed from config
let seed = trainer.training_seed();
```

### Cancellation

```rust
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

let cancel_token = Arc::new(AtomicBool::new(false));
trainer.set_cancel_token(cancel_token.clone());

// In another task:
cancel_token.store(true, Ordering::SeqCst);

// Training checks at epoch boundaries and stops gracefully
```

### Metrics Persistence

```rust
trainer.set_job_id("job-123".into());
trainer.set_db(db);

// Trainer persists epoch metrics to repository_training_metrics table
```

## File Locations

| Component | Path |
|-----------|------|
| Trainer | `trainer.rs`, `trainer/types.rs` |
| Config types | `trainer/types.rs:213-623` |
| Checkpoint | `checkpoint.rs` |
| Dataset builder | `builder.rs` |
| Quantizer | `quantizer.rs` |
| Packager | `packager/mod.rs`, `packager/aos.rs` |
| Loss functions | `loss.rs`, `embedding_loss.rs` |
| LR schedules | `learning_rate_schedule.rs` |
| Early stopping | `early_stopping.rs` |
| Dataset split | `dataset.rs` |
