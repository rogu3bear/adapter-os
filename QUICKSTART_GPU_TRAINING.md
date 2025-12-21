# Quick Start: GPU Training for AdapterOS

## Overview

AdapterOS now supports GPU-accelerated LoRA training with automatic backend selection:
- **CoreML/ANE** (best power efficiency)
- **Metal GPU** (universal on macOS)
- **MLX** (research/training)
- **CPU** (fallback)

## 5-Minute Setup

### 1. Create Trainer (Auto-Select GPU)

```rust
use adapteros_lora_worker::training::{MicroLoRATrainer, TrainingConfig};

let config = TrainingConfig::default();
let mut trainer = MicroLoRATrainer::new(config)?;

// Automatically selects best available backend
trainer.init_kernels(&plan_bytes)?;
```

### 2. Check GPU Status

```rust
if trainer.using_gpu() {
    println!("Training with: {}", trainer.backend_info().unwrap());
} else {
    println!("Training on CPU");
}
```

### 3. Run Training

```rust
let result = trainer.train(&examples).await?;
println!("Final loss: {}", result.final_loss);
println!("Time: {}ms", result.training_time_ms);
```

## Configuration Options

### Option A: Default (Auto-Select)
```rust
let config = TrainingConfig::default();
```

### Option B: Require GPU
```rust
let config = TrainingConfig::default()
    .with_gpu_required();
// Errors if GPU unavailable
```

### Option C: Specify Backend
```rust
use adapteros_lora_worker::training::TrainingBackend;

let config = TrainingConfig::default()
    .with_backend(TrainingBackend::Metal);
```

### Option D: Set Memory Limit
```rust
let config = TrainingConfig::default()
    .with_max_gpu_memory(2048);  // 2GB max
```

### Option E: Chain Options
```rust
let config = TrainingConfig::default()
    .with_backend(TrainingBackend::CoreML)
    .with_gpu_required()
    .with_max_gpu_memory(4096);
```

## Common Use Cases

### Use Case 1: Production Training (GPU Required)
```rust
let config = TrainingConfig::default()
    .with_gpu_required()
    .with_backend(TrainingBackend::CoreML);

let mut trainer = MicroLoRATrainer::new(config)?;
trainer.init_kernels(&plan)?;
let result = trainer.train(&examples).await?;
```

### Use Case 2: Research Training (GPU Optional)
```rust
let config = TrainingConfig::default()
    .with_backend(TrainingBackend::Metal);

let mut trainer = MicroLoRATrainer::new(config)?;
trainer.init_kernels(&plan)?;  // Falls back to CPU if GPU fails

if trainer.using_gpu() {
    println!("Using Metal GPU");
} else {
    println!("Using CPU");
}

let result = trainer.train(&examples).await?;
```

### Use Case 3: Development (CPU Only)
```rust
let config = TrainingConfig::default()
    .with_backend(TrainingBackend::Cpu);

let mut trainer = MicroLoRATrainer::new(config)?;
trainer.init_kernels(&[])?;  // No GPU initialization

let result = trainer.train(&examples).await?;
```

### Use Case 4: Mobile/Edge (Auto-Select with Limit)
```rust
let config = TrainingConfig::default()
    .with_max_gpu_memory(512);  // 512MB max

let mut trainer = MicroLoRATrainer::new(config)?;
trainer.init_kernels(&plan)?;

let result = trainer.train(&examples).await?;
```

## Error Handling

### GPU Required but Unavailable
```rust
let config = TrainingConfig::default()
    .with_gpu_required();

let mut trainer = MicroLoRATrainer::new(config)?;
match trainer.init_kernels(&plan) {
    Ok(_) => println!("GPU ready"),
    Err(e) => {
        eprintln!("GPU failed: {}", e);
        // Error shows available backends
    }
}
```

### Optional GPU Fallback
```rust
let config = TrainingConfig::default()
    .with_backend(TrainingBackend::Metal);

let mut trainer = MicroLoRATrainer::new(config)?;
trainer.init_kernels(&plan)?;  // Succeeds even if GPU fails

// Check what we're actually using
if trainer.using_gpu() {
    println!("GPU: {}", trainer.backend_info().unwrap());
} else {
    println!("CPU fallback (GPU unavailable)");
}
```

## Monitoring Performance

### Check Backend in Use
```rust
println!("Backend: {}", trainer.backend_info().unwrap_or("CPU"));
println!("Using GPU: {}", trainer.using_gpu());
```

### Track Throughput
```rust
let examples_per_second = examples.len() as f32 / (result.training_time_ms as f32 / 1000.0);
println!("Throughput: {:.0} ex/sec", examples_per_second);
```

### Compare with CPU
```rust
// Train with GPU
let config_gpu = TrainingConfig::default()
    .with_backend(TrainingBackend::Metal);
let mut trainer_gpu = MicroLoRATrainer::new(config_gpu)?;
trainer_gpu.init_kernels(&plan)?;
let result_gpu = trainer_gpu.train(&examples).await?;

// Train with CPU
let config_cpu = TrainingConfig::default()
    .with_backend(TrainingBackend::Cpu);
let mut trainer_cpu = MicroLoRATrainer::new(config_cpu)?;
trainer_cpu.init_kernels(&[])?;
let result_cpu = trainer_cpu.train(&examples).await?;

let speedup = result_cpu.training_time_ms as f32 / result_gpu.training_time_ms as f32;
println!("GPU speedup: {:.1}x", speedup);
```

## Telemetry Events

Training events are automatically logged:

- **training.started** - Training begun (includes backend info)
- **training.epoch_completed** - Epoch finished with loss
- **training.completed** - Training done (with throughput metrics)
- **training.backend_selected** - Backend chosen
- **training.gpu_fallback** - GPU fallback triggered

Example telemetry:
```json
{
    "event": "training.completed",
    "backend": "Metal",
    "using_gpu": true,
    "training_time_ms": 15234,
    "performance": {
        "examples_per_second": 6.55,
        "total_examples": 100,
        "final_loss": 0.0234
    }
}
```

## Troubleshooting

### GPU Not Detected
```rust
let backends = MicroLoRATrainer::describe_available_backends();
println!("{}", backends);
```

Output shows available options.

### GPU Initialization Failed
Check error message which includes:
1. Which backend failed
2. Why it failed
3. Available alternatives

### Slow Training
- Check `trainer.using_gpu()` to confirm GPU active
- Review `backend_info()` for backend name
- Compare with CPU baseline using benchmarks

## Best Practices

1. **Always check `using_gpu()`** - Verify GPU active before assumptions
2. **Use `with_gpu_required()`** for critical workloads - Fail fast if GPU unavailable
3. **Set memory limits** for embedded systems - Prevents OOM scenarios
4. **Log backend selection** - Helps with debugging and monitoring
5. **Compare baselines** - Benchmark CPU vs GPU for your workloads
6. **Monitor telemetry** - Track throughput and performance trends

## Platform Support

| Feature | macOS | Linux | Windows |
|---------|-------|-------|---------|
| CoreML/ANE | ✓ | ✗ | ✗ |
| Metal | ✓ | ✗ | ✗ |
| MLX | ✓ | ✗ | ✗ |
| CPU | ✓ | ✓ | ✓ |

## Documentation

- **API Reference:** `docs/GPU_TRAINING_INTEGRATION.md`
- **Architecture:** `docs/ARCHITECTURE.md#architecture-components`
- **Training Pipeline:** `docs/TRAINING_PIPELINE.md`

## Running Tests

```bash
# Unit tests
cargo test -p adapteros-lora-worker 'training::trainer::tests'

# Integration tests
cargo test --test gpu_training_integration

# Benchmarks
cargo run --bin gpu_training_benchmark --release
```

## Next Steps

1. Review `docs/GPU_TRAINING_INTEGRATION.md` for complete API
2. Run integration tests to verify setup
3. Run benchmarks to profile your system
4. Integrate into your training pipeline
5. Monitor telemetry for performance insights

---

## See Also

- [docs/GPU_TRAINING_INTEGRATION.md](docs/GPU_TRAINING_INTEGRATION.md) - Complete GPU training API reference
- [docs/TRAINING_PIPELINE.md](docs/TRAINING_PIPELINE.md) - Training pipeline architecture
- [docs/ARCHITECTURE.md#architecture-components](docs/ARCHITECTURE.md#architecture-components) - Architectural patterns and diagrams
- [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md) - MLX backend for research/training
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML backend with ANE acceleration
- [docs/FEATURE_FLAGS.md](docs/FEATURE_FLAGS.md) - Feature flag reference for backend selection
- [QUICKSTART.md](QUICKSTART.md) - Quick start guide for inference
- [AGENTS.md](AGENTS.md) - Developer quick reference guide

---

**Last Updated:** 2025-11-21
**Status:** Production Ready
