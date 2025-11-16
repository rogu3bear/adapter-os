# MLX C++ FFI Integration

This document explains how the MLX (Apple) C++ FFI path is detected, built, and used in AdapterOS, and how to verify whether you have a real integration or a stub build.

**Note:** MLX backend uses pure C++ FFI - no Python or PyO3 required. The backend is production-ready and can be enabled via the `mlx-ffi-backend` feature flag.

## Feature Flag

MLX backend is enabled via the `mlx-ffi-backend` feature flag:

```bash
# Build with MLX backend support
cargo build --release --features mlx-ffi-backend

# Or for development
cargo build --features mlx-ffi-backend
```

The `mlx-ffi-backend` feature is independent of `experimental-backends` and does not require PyO3.

## Build Modes

- REAL: `mlx_real` cfg. Build script found MLX C++ headers, compiled wrapper with `-DMLX_HAVE_REAL_API`, and linked `-lmlx`.
- STUB: `mlx_stub` cfg. Build script did not find headers (or `MLX_FORCE_STUB=1` was set). The wrapper compiles to a self-contained stub with deterministic placeholders.

The build emits clear logs:
- `MLX FFI build: REAL` with selected include/lib paths
- `MLX FFI build: STUB` with a reason and remediation hints

## Environment Variables (precedence)

1. `MLX_INCLUDE_DIR` and `MLX_LIB_DIR` — explicit include/lib locations
2. `MLX_PATH` — base directory; we use `MLX_PATH/include` and `MLX_PATH/lib`
3. Defaults — `/opt/homebrew/include` and `/opt/homebrew/lib`

<<<<<<< HEAD
Optional:
- `MLX_FORCE_STUB=1` — force a stub build (useful for CI and tests)
=======
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
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

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
>>>>>>> integration-branch

## Configuration

### Environment Variables

Set the model path via environment variable:

```bash
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
```

### Configuration File

Add MLX configuration to `configs/cp.toml`:

```toml
[mlx]
# Enable MLX backend support (requires --features mlx-ffi-backend)
enabled = true
# Default model path (can be overridden by AOS_MLX_FFI_MODEL env var)
model_path = "./models/qwen2.5-7b-mlx"
# Default backend selection when both Metal and MLX are available
# Options: "metal" (default, production) or "mlx" (development/experimentation)
default_backend = "mlx"
```

If `model_path` is set in config and `AOS_MLX_FFI_MODEL` is also set, the environment variable takes precedence.

## Runtime Guards

- `MLXFFIModel::load(..)` returns `AosError::Unsupported` on stub builds with a helpful message. This prevents silent use of placeholder outputs during inference.
- The CLI (`aosctl serve --backend mlx`) also fails fast on stub builds with actionable guidance.
- The import command (`aosctl import-model`) validates MLX models and sets the environment variable automatically.

## Verifying Your Setup

- Inspect build output for the `MLX FFI build:` line.
- On the Rust side, `cfg!(mlx_real)` indicates a real build; `cfg!(mlx_stub)` indicates a stub build.
- Through the FFI, `mlx_wrapper_is_real()` returns 1 for real builds and 0 for stub builds.

## Common Issues

- Headers found, but link fails: ensure `MLX_LIB_DIR` is correct and contains `libmlx`.
- Partial installs: when only `MLX_PATH` is set but headers aren’t there, the build falls back to stub and logs why.
- ABI drift: even if linking succeeds, runtime symbol issues can occur with newer MLX releases. Validate with a small smoke test calling `mlx_model_load`/`mlx_model_free`.

## Troubleshooting Matrix

- Unset env → Stub (expected). Set `MLX_INCLUDE_DIR/MLX_LIB_DIR` to switch to real.
- Only `MLX_PATH` set → Uses `MLX_PATH/include` and `MLX_PATH/lib`.
- Conflicting values → `MLX_INCLUDE_DIR/MLX_LIB_DIR` win over `MLX_PATH`.

## Usage Examples

### Import MLX Model

```bash
# Import MLX model (requires --features mlx-ffi-backend)
./target/release/aosctl import-model \
  --name qwen2.5-7b-mlx \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
  --license models/qwen2.5-7b-mlx/LICENSE
```

### Serve with MLX Backend

```bash
# Set model path
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx

# Start server with MLX backend
./target/release/aosctl serve --backend mlx --model-path ./models/qwen2.5-7b-mlx
```

### Launch Script Support

```bash
# Launch backend with MLX
./launch.sh backend mlx ./models/qwen2.5-7b-mlx
```

## Notes

- **No PyO3 required** - MLX backend uses pure C++ FFI, no Python runtime needed
- The wrapper currently retains stub logic under real mode as a placeholder; actual MLX C++ calls can be introduced behind `#ifdef MLX_HAVE_REAL_API` with no changes to the Rust ABI
- MLX backend is production-ready and can be used alongside Metal backend
- Feature flag `mlx-ffi-backend` is independent and does not require `experimental-backends`
