# MLX Integration Guide

## Overview

MPLoRA integrates with Apple's MLX framework to provide high-performance inference on Apple Silicon with memory-parallel LoRA routing.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       MPLoRA System                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │   CLI Tool   │───▶│  MLX Backend │───▶│  MLX Model   │ │
│  │   (aosctl)   │    │              │    │  (Python)    │ │
│  └──────────────┘    └──────────────┘    └──────────────┘ │
│         │                    │                    │         │
│         │                    │                    │         │
│         ▼                    ▼                    ▼         │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │   Registry   │    │   K-Sparse   │    │  LoRA        │ │
│  │   Database   │    │   Router     │    │  Adapters    │ │
│  └──────────────┘    └──────────────┘    └──────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Prerequisites

### System Requirements
- **macOS** with Apple Silicon (M1, M2, M3, M4)
- **Python 3.9+** (Python 3.13 recommended)
- **Rust 1.70+**
- **Xcode Command Line Tools**

### Python Dependencies

Install MLX and related packages:

```bash
pip install -r requirements-mlx.txt
```

Or manually:

```bash
pip install mlx>=0.10.0 numpy>=1.24.0 safetensors>=0.4.0
```

## Backend Selection

AdapterOS supports multiple inference backends that can be selected at runtime:

- **Metal**: Native Metal GPU backend for maximum performance on macOS
- **MLX**: Python/MLX backend for flexibility and experimentation

### Selecting a Backend

Use the `--backend` flag when starting the server:

```bash
# Use Metal backend (default)
aosctl serve --tenant my_tenant --plan my_plan --backend metal

# Use MLX backend
aosctl serve --tenant my_tenant --plan my_plan --backend mlx
```

### Backend Comparison

| Feature | Metal | MLX |
|---------|-------|-----|
| Performance | Highest | Good |
| Setup | Automatic on macOS | Requires Python/MLX |
| LoRA Support | Full | Full |
| Adapter Hot-Swap | Limited | Full |
| Model Format | .metallib | .safetensors |

### When to Use Each Backend

**Use Metal when:**
- Maximum performance is required
- Running in production
- Model is pre-compiled to Metal shaders

**Use MLX when:**
- Experimenting with new models
- Rapid prototyping
- Need dynamic adapter loading
- Python ecosystem integration needed

## Setup

### 1. Build MPLoRA

```bash
# Build all workspace crates (libraries only)
DATABASE_URL=sqlite://var/aos.db cargo build --workspace --lib

# Build specific crates
cargo build --package mplora-mlx
cargo build --package mplora-server
```

**Note**: The CLI binary (`aosctl`) requires Python runtime linking for MLX integration. Python runtime is initialized automatically when using the MLX backend.

### 2. Prepare Model Files

MPLoRA expects models in MLX format with the following structure:

```
models/qwen2.5-7b-mlx/
├── config.json          # Model configuration
├── model.safetensors    # Model weights
├── tokenizer.json       # Tokenizer
└── tokenizer_config.json
```

#### Converting Models to MLX Format

Use the official MLX conversion tools:

```bash
# Install mlx-lm
pip install mlx-lm

# Convert a Hugging Face model
python -m mlx_lm.convert \
    --hf-path Qwen/Qwen2.5-7B-Instruct \
    --mlx-path models/qwen2.5-7b-mlx
```

### 3. Prepare LoRA Adapters

LoRA adapters should be in `.safetensors` format with the following tensor naming convention:

```
{module_name}.lora_A  # Down-projection matrix [rank, in_features]
{module_name}.lora_B  # Up-projection matrix [out_features, rank]
```

Example modules:
- `q_proj.lora_A`, `q_proj.lora_B`
- `k_proj.lora_A`, `k_proj.lora_B`
- `v_proj.lora_A`, `v_proj.lora_B`
- `o_proj.lora_A`, `o_proj.lora_B`

## Usage

### Loading a Model

```rust
use mplora_mlx::MLXModel;

// Load model from directory
let model = MLXModel::load("models/qwen2.5-7b-mlx")?;

// Get model info
println!("Hidden size: {}", model.hidden_size());
println!("Vocab size: {}", model.vocab_size());
```

### Loading LoRA Adapters

```rust
use mplora_mlx::lora::{LoRAAdapter, LoRAConfig};

// Configure LoRA
let config = LoRAConfig {
    rank: 16,
    alpha: 32.0,
    target_modules: vec![
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "o_proj".to_string(),
    ],
    dropout: 0.0,
};

// Load adapter
let adapter = LoRAAdapter::load(
    "adapters/my_adapter.safetensors",
    "my_adapter".to_string(),
    config,
)?;

println!("Loaded {} modules", adapter.num_modules());
```

### K-Sparse Routing

```rust
use mplora_mlx::routing::{select_top_k, apply_multi_lora};

// Router produces logits for each adapter
let router_logits = vec![2.5, 1.2, 3.1, 0.8, 2.0];

// Select top-3 adapters
let (indices, gates) = select_top_k(&router_logits, 3);
// indices: [2, 0, 4] (highest logits)
// gates: [15234, 10892, 6641] (Q15 quantized, sum ≈ 32767)

// Apply adapters with gates
let adapters = vec![&adapter1, &adapter2, &adapter3];
let output = apply_multi_lora(
    &adapters,
    &gates,
    "q_proj",
    &input_activations,
    &base_output,
)?;
```

### MLX Backend for Inference

```rust
use mplora_mlx::{MLXModel, MLXBackend, LoRAAdapter};
use mplora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

// Load model and create backend
let model = MLXModel::load("models/qwen2.5-7b-mlx")?;
let mut backend = MLXBackend::new(model);

// Register adapters
backend.register_adapter(0, adapter1)?;
backend.register_adapter(1, adapter2)?;
backend.register_adapter(2, adapter3)?;

// Prepare inference
let mut io = IoBuffers::new(vocab_size);
io.input_ids = vec![1, 2, 3]; // token IDs

// Router decision (from K-sparse router)
let mut ring = RouterRing::new(3);
ring.set(&[0, 1, 2], &[15000, 10000, 7767]); // adapter IDs and Q15 gates

// Run inference step
backend.run_step(&ring, &mut io)?;

// Output logits are in io.output_logits
```

## Configuration

### Model Config (config.json)

```json
{
  "hidden_size": 4096,
  "num_hidden_layers": 32,
  "num_attention_heads": 32,
  "num_key_value_heads": 32,
  "intermediate_size": 11008,
  "vocab_size": 151936,
  "max_position_embeddings": 32768,
  "rope_theta": 10000.0
}
```

### LoRA Config

```rust
LoRAConfig {
    rank: 16,              // LoRA rank (typical: 8, 16, 32, 64)
    alpha: 32.0,           // Scaling factor (typical: 2 * rank)
    target_modules: vec![  // Modules to adapt
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "o_proj".to_string(),
    ],
    dropout: 0.0,          // Dropout (usually 0 for inference)
}
```

### Router Config

```rust
// K-sparse routing
let k = 3;  // Number of active adapters (1, 3, 5, or 8)

// Entropy floor (prevent single-adapter collapse)
let entropy_floor = 0.02;  // 0.0 to 1.0

// Apply entropy floor to gates
let adjusted_gates = apply_entropy_floor(&gates, entropy_floor);
```

## Performance Optimization

### 1. Memory Management

MLX uses unified memory on Apple Silicon:

```rust
// Monitor adapter count
let adapter_count = backend.adapter_count();

// Evict adapters when memory pressure detected
// (handled automatically by MPLoRA memory monitor)
```

### 2. Batch Size

Start with small batch sizes and increase:

```rust
// Single token (autoregressive generation)
let batch_size = 1;

// Batch processing (prompt encoding)
let batch_size = 4;  // Adjust based on memory
```

### 3. Quantization

MLX supports various quantization schemes:

- **Q15 Gates**: Router gates quantized to 16-bit (0-32767)
- **FP16 Weights**: Model weights in half precision
- **INT8 Activations**: Quantized activations (future)

### 4. Profiling

Use macOS Instruments to profile:

```bash
# Build with release profile
cargo build --release --package mplora-mlx

# Profile with Instruments
instruments -t "Time Profiler" ./target/release/your_binary
```

## Troubleshooting

### Python Linking Errors

**Problem**: `ld: symbol(s) not found for architecture arm64`

**Solution**: PyO3 requires Python runtime. Use library API instead of CLI binary:

```rust
// In your Rust code
use mplora_mlx::MLXModel;

fn main() {
    pyo3::prepare_freethreaded_python();
    let model = MLXModel::load("models/qwen2.5-7b-mlx").unwrap();
    // ...
}
```

### MLX Import Errors

**Problem**: `Failed to import mlx.core`

**Solution**: Ensure MLX is installed:

```bash
pip install --upgrade mlx
python -c "import mlx.core; print(mlx.core.__version__)"
```

### Model Loading Failures

**Problem**: `Failed to load MLX model`

**Solution**: Verify model files:

```bash
ls -lh models/qwen2.5-7b-mlx/
# Should show: config.json, model.safetensors, tokenizer.json
```

### LoRA Shape Mismatches

**Problem**: `Shape mismatch: expected [X, Y], got [A, B]`

**Solution**: Ensure LoRA rank matches:

```rust
// Check adapter rank
let config = LoRAConfig {
    rank: 16,  // Must match the rank used during training
    // ...
};
```

### Determinism Issues

**Problem**: Different outputs with same input

**Solution**: Ensure deterministic seeding:

```rust
// Use HKDF-based seeding (built into MPLoRA)
// Avoid using system random number generators
```

## Advanced Topics

### Custom LoRA Targets

Target different modules:

```rust
let config = LoRAConfig {
    rank: 16,
    alpha: 32.0,
    target_modules: vec![
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "o_proj".to_string(),
        "gate_proj".to_string(),  // MLP gate
        "up_proj".to_string(),    // MLP up
        "down_proj".to_string(),  // MLP down
    ],
    dropout: 0.0,
};
```

### Multi-Adapter Composition

Combine multiple adapters:

```rust
// Load domain-specific adapters
let medical_adapter = LoRAAdapter::load("adapters/medical.safetensors", ...)?;
let legal_adapter = LoRAAdapter::load("adapters/legal.safetensors", ...)?;
let code_adapter = LoRAAdapter::load("adapters/code.safetensors", ...)?;

// Router decides which to activate based on input
let (indices, gates) = router.route(&input)?;

// Apply selected adapters
let output = apply_multi_lora(&selected_adapters, &gates, ...)?;
```

### Entropy Floor Tuning

Prevent adapter collapse:

```rust
// Low entropy floor: allow specialization
let entropy_floor = 0.01;

// High entropy floor: force diversity
let entropy_floor = 0.10;

// Apply to gates
let adjusted_gates = apply_entropy_floor(&gates, entropy_floor);
```

## Testing

### Unit Tests

```bash
# Run tests (no MLX required)
cargo test --package mplora-mlx --lib test_model_config_parsing
cargo test --package mplora-mlx --lib test_lora_apply_simple
cargo test --package mplora-mlx --lib test_select_top_k
```

### Integration Tests (Requires MLX)

```bash
# Install MLX
pip install -r requirements-mlx.txt

# Run integration tests
cargo test --package mplora-mlx -- --ignored
```

## References

- [MLX Documentation](https://ml-explore.github.io/mlx/)
- [LoRA Paper](https://arxiv.org/abs/2106.09685)
- [Qwen2.5 Models](https://huggingface.co/Qwen)
- [SafeTensors Format](https://github.com/huggingface/safetensors)

## Support

For issues and questions:
- GitHub Issues: [adapter-os/issues](https://github.com/rogu3bear/adapter-os/issues)
- Documentation: [README.md](../README.md)
- Status: [MLX_INTEGRATION_STATUS.md](../MLX_INTEGRATION_STATUS.md)

---

**Last Updated**: 2025-10-07  
**Version**: 0.1.0  
**Status**: Phase 1-5 Complete ✅
