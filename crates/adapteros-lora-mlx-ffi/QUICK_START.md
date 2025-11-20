# Quick Start: Shared Down Projection Architecture

## Creating an Adapter with Shared Down Projection

```rust
use adapteros_lora_mlx_ffi::{LoRAAdapter, LoRAConfig};

// 1. Define your LoRA configuration
let config = LoRAConfig {
    rank: 16,
    alpha: 32.0,
    target_modules: vec![
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "o_proj".to_string(),
    ],
    dropout: 0.1,
};

// 2. Create shared down projection (rank × hidden_dim)
let rank = 16;
let hidden_dim = 4096;
let shared_down = vec![vec![0.0; hidden_dim]; rank];

// 3. Initialize adapter with shared down
let mut adapter = LoRAAdapter::new_with_shared_down(
    "my-adapter".to_string(),
    config,
    shared_down,
);

// 4. Add per-module up projections (hidden_dim × rank)
for module in &["q_proj", "k_proj", "v_proj", "o_proj"] {
    let lora_b = vec![vec![0.0; rank]; hidden_dim];
    adapter.add_module_weights(module, lora_b);
}
```

## Loading from .aos File

```rust
use adapteros_lora_mlx_ffi::{LoRAAdapter, LoRAConfig};

let adapter = LoRAAdapter::load(
    "./my-adapter.aos",
    "my-adapter".to_string(),
    config,
)?;
```

## Memory Tracking

```rust
// Get memory usage
let memory_bytes = adapter.memory_usage();
let memory_mb = memory_bytes as f32 / (1024.0 * 1024.0);

// Get detailed breakdown
let breakdown = adapter.memory_breakdown();
for (tensor_name, bytes) in breakdown {
    println!("{}: {} bytes", tensor_name, bytes);
}

// Get parameter count
let params = adapter.parameter_count();
let tensors = adapter.tensor_count();
```

## Applying LoRA to Inference

```rust
use adapteros_lora_mlx_ffi::routing::apply_multi_lora;

// Get weights for a module
if let Some((shared_down, lora_b)) = adapter.get_full_weights("q_proj") {
    // Apply transformation
    let output = apply_lora_transform_shared(
        input_activations,
        shared_down,
        lora_b,
        adapter.config().alpha,
    )?;
}
```

## Expected Safetensors Layout

Your `.aos` file should contain tensors with these keys:

```
lora.shared_down          [16, 4096]     # Shared down projection
lora.q_proj.up            [4096, 16]     # Q projection up
lora.k_proj.up            [4096, 16]     # K projection up
lora.v_proj.up            [4096, 16]     # V projection up
lora.o_proj.up            [4096, 16]     # O projection up
```

## Memory Savings Example

For a 7B model with 4 target modules (q/k/v/o):

**Traditional LoRA:**
```
4 modules × 2 matrices × 16 rank × 4096 hidden_dim = 524,288 parameters
```

**Shared Down LoRA:**
```
1 shared_down × 16 rank × 4096 hidden_dim = 65,536 parameters
4 modules × 1 matrix × 16 rank × 4096 hidden_dim = 262,144 parameters
Total = 327,680 parameters (37.5% savings!)
```

## Testing

```bash
# Run all tests
cargo test -p adapteros-lora-mlx-ffi --lib

# Run specific test
cargo test -p adapteros-lora-mlx-ffi test_lora_adapter_shared_architecture
```

## Common Patterns

### Check if module has weights
```rust
if adapter.has_module("q_proj") {
    // Module is ready
}
```

### Get tensor shapes
```rust
if let Some((rank, hidden_dim)) = adapter.shared_down_shape() {
    println!("Shared down: {}×{}", rank, hidden_dim);
}
```

### Validate adapter
```rust
assert!(adapter.shared_down().is_some());
assert_eq!(adapter.tensor_count(), 5); // 1 shared + 4 modules
```
