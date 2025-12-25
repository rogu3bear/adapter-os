# MoE Training Quick Reference

Quick reference for training LoRA adapters on Mixture of Experts (MoE) models.

**Last Updated:** 2025-12-25

---

## Overview

AdapterOS supports training LoRA adapters for MoE models like **Qwen3-Coder-30B-A3B** (128 experts, 8 active per token). The implementation uses **routing-weighted shared LoRA** - a single set of LoRA weights scaled by expert routing weights.

### Key Formula

```
lora_out = sum(routing_weight[e]) * (alpha/rank) * (B @ A) @ x
```

Where:
- `routing_weight[e]` = Q15 routing weight for expert `e` (normalized, sums to ~1.0)
- `alpha/rank` = LoRA scaling factor
- `B @ A` = LoRA delta matrix (precomputed for inference)
- `x` = input hidden state

---

## Quick Start

### 1. Configure MoE Training

```rust
use adapteros_lora_worker::training::{TrainingConfig, MoELoRAStrategy};

// Simple configuration
let config = TrainingConfig::default()
    .with_moe(128, 8)   // 128 experts, 8 per token
    .with_rank(4)
    .with_alpha(8.0)
    .with_epochs(3)
    .with_backend(TrainingBackend::CoreML);
```

### 2. Train Adapter

```rust
let trainer = MicroLoRATrainer::new(config)?;
let result = trainer.train(&examples, "qwen3-moe-adapter-v1").await?;

println!("Final loss: {:.4}", result.final_loss);
println!("Is MoE: {}", result.weights.is_moe());
```

### 3. Package as .aos

The packager automatically includes `moe_config` in the manifest when training config has MoE enabled.

```rust
let packager = AdapterPackager::new(repo_root);
let package = packager.package_aos(
    &result.weights,
    &config,
    "Qwen/Qwen3-Coder-30B-A3B",
    &metadata,
).await?;
```

---

## Configuration Reference

### MoETrainingConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `num_experts` | `usize` | Required | Total experts in model (e.g., 128) |
| `num_experts_per_token` | `usize` | Required | Active experts per token (e.g., 8) |
| `lora_strategy` | `MoELoRAStrategy` | `RoutingWeightedShared` | LoRA strategy |
| `use_routing_weights` | `bool` | `true` | Scale by routing weights |
| `moe_intermediate_size` | `Option<usize>` | `None` | Expert intermediate dimension |

### LoRA Strategies

| Strategy | Description |
|----------|-------------|
| `RoutingWeightedShared` | Single LoRA scaled by routing weights (default) |
| `PerExpertLoRA` | Separate LoRA per expert (future) |

---

## .aos Manifest Format

MoE adapters include `moe_config` in the manifest:

```json
{
  "version": "2.0",
  "rank": 4,
  "alpha": 8.0,
  "base_model": "Qwen/Qwen3-Coder-30B-A3B",
  "moe_config": {
    "num_experts": 128,
    "num_experts_per_token": 8,
    "lora_strategy": "routing_weighted_shared",
    "use_routing_weights": true,
    "moe_intermediate_size": 768
  },
  "quantization": "q15",
  "gate_q15_denominator": 32767
}
```

---

## Loading MoE Adapters

### Basic Load with Validation

```rust
use adapteros_aos::AosManager;

let manager = AosManager::builder()
    .with_cache(1024 * 1024 * 1024)
    .with_hot_swap()
    .build()?;

// Load and validate as MoE
let adapter = manager.load_moe(&path).await?;
let config = adapter.moe_config().unwrap();
println!("Experts: {}", config.num_experts);
```

### Validate Against Expected Config

```rust
use adapteros_aos::MoEConfigManifest;

let expected = MoEConfigManifest {
    num_experts: 128,
    num_experts_per_token: 8,
    lora_strategy: "routing_weighted_shared".to_string(),
    use_routing_weights: true,
    ..Default::default()
};

// Fails if expert count mismatches
let adapter = manager.load_moe_validated(&path, &expected).await?;
```

### Discover Compatible Adapters

```rust
// Find all MoE adapters
let all_moe = manager.discover_moe_adapters(&adapters_dir).await?;

// Find adapters matching specific config
let compatible = manager.find_compatible_moe_adapters(&adapters_dir, &expected).await?;
```

---

## Hot-Swap Operations

### Swap MoE Adapter

```rust
// Hot-swap with validation
manager.hot_swap_moe("slot1", &new_adapter_path).await?;

// Get active MoE adapters
for (slot, adapter) in manager.get_active_moe_adapters() {
    println!("{}: {} experts", slot, adapter.moe_config().unwrap().num_experts);
}
```

### Memory Management

```rust
// Check MoE memory usage
let bytes = manager.moe_cache_size_bytes();
let count = manager.moe_cache_count();

// Evict MoE adapters if needed
let freed = manager.evict_moe_adapters();
```

---

## Best Practices

### 1. Match Expert Configuration
```rust
// Verify before training
assert_eq!(config.moe_config.as_ref().unwrap().num_experts, 128);
assert_eq!(config.moe_config.as_ref().unwrap().num_experts_per_token, 8);
```

### 2. Use Validation on Load
```rust
// Always validate MoE config matches target model
let adapter = manager.load_moe_validated(&path, &model_moe_config).await?;
```

### 3. Monitor Memory
```rust
// MoE adapters can be large
if manager.moe_cache_size_bytes() > max_cache_bytes {
    manager.evict_moe_adapters();
}
```

### 4. Use Routing Weights
```rust
// Enable routing weights for proper expert scaling
let config = MoETrainingConfig {
    use_routing_weights: true,  // Important!
    ..
};
```

---

## Error Handling

### Expert Count Mismatch

```
Error: MoE expert count mismatch: adapter has 128 experts, expected 64
```

**Fix:** Ensure adapter was trained for the correct model configuration.

### Dense Adapter Loaded as MoE

```
Error: Adapter 'dense-adapter' is not configured for MoE models
```

**Fix:** Use `manager.load()` for dense adapters, `load_moe()` for MoE.

### Missing MoE Config in Manifest

**Fix:** Retrain with `config.with_moe(num_experts, experts_per_token)`.

---

## Related Documentation

- [TRAINING.md](TRAINING.md) - Complete training guide
- [AOS_FORMAT.md](AOS_FORMAT.md) - .aos archive specification
- [COREML_MOE_IMPLEMENTATION.md](COREML_MOE_IMPLEMENTATION.md) - CoreML MoE backend

---

MLNavigator Inc 2025-12-25
