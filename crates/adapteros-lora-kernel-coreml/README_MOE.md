# CoreML MoE Support

This crate now includes native support for Mixture of Experts (MoE) models running on Apple Neural Engine via CoreML.

## Quick Start

### 1. Convert Your MoE Model

Convert an MLX-format MoE model to CoreML `.mlpackage`:

```bash
python scripts/convert_mlx_to_coreml.py \
  --model-path ./var/models/Qwen3-30B-Instruct-MLX \
  --output-path ./var/models/Qwen3-30B-CoreML.mlpackage \
  --compute-units ALL
```

### 2. Load and Run

```rust
use adapteros_lora_kernel_coreml::{CoreMLBackend, ComputeUnits};

// Create backend
let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndNeuralEngine, false)?;

// Load MoE model - detection happens automatically
backend.load_model(Path::new("./var/models/Qwen3-30B-CoreML.mlpackage"))?;

// Check if MoE
if backend.is_moe_model() {
    if let Some(config) = backend.moe_config() {
        println!("Loaded MoE model: {}", config.description());
        // Output: "MoE: 60 experts + 8 shared, top-8 routing, hidden=3584, intermediate=1408"
    }
}

// Run inference (works the same as dense models)
backend.run_step(&mut ring, &mut io)?;
```

## Features

### Automatic MoE Detection

The backend automatically detects MoE architecture from `config.json`:

```json
{
  "architectures": ["Qwen2MoeForCausalLM"],
  "num_experts": 60,
  "num_experts_per_tok": 8,
  "hidden_size": 3584,
  "moe_intermediate_size": 1408,
  "num_shared_experts": 8
}
```

**Supported Fields**:
- `num_experts` or `num_local_experts` - Total expert count
- `num_experts_per_tok` or `num_experts_per_token` or `top_k` - Experts per token
- `hidden_size` - Hidden dimension
- `moe_intermediate_size` or `intermediate_size` - Expert FFN size
- `num_shared_experts` (optional) - Shared expert count

### Native CoreML Runtime

Expert routing and inference are handled entirely by CoreML's optimized runtime:

- **ANE Acceleration**: Expert routing and FFN computations run on Neural Engine
- **Zero Overhead**: No custom routing logic or IPC overhead
- **Memory Efficient**: CoreML handles expert weight loading on-demand

### Transparent API

MoE models work with the same `FusedKernels` API as dense models:

```rust
impl FusedKernels for CoreMLBackend {
    fn forward_base(&mut self, input_ids: &[u32]) -> Result<Vec<f32>> {
        // Works for both dense and MoE models
    }

    // Note: forward_lora() not yet supported for MoE models
}
```

## Validation

The backend validates MoE configurations on load:

```rust
pub fn validate(&self) -> Result<()> {
    if self.num_experts == 0 {
        return Err(AosError::Config("MoE num_experts must be greater than 0"));
    }

    if self.num_experts_per_tok > self.num_experts {
        return Err(AosError::Config(format!(
            "num_experts_per_tok ({}) cannot exceed num_experts ({})",
            self.num_experts_per_tok, self.num_experts
        )));
    }

    // ... additional validation
    Ok(())
}
```

Invalid configurations are logged and the model is treated as dense.

## Performance

### Apple Neural Engine Benefits

MoE models on ANE:
- **Power Efficient**: ANE is 10x more power-efficient than GPU for inference
- **Low Latency**: Reduced memory bandwidth usage with on-chip expert weights
- **Parallel Execution**: Multiple experts can run concurrently on ANE

### Compute Unit Recommendations

```rust
// Recommended: Deterministic + power-efficient
ComputeUnits::CpuAndNeuralEngine

// Maximum performance (may use GPU for some ops)
ComputeUnits::All

// Debug/testing only
ComputeUnits::CpuOnly
```

## Limitations

### Current (Phase 3)

1. **No LoRA Fusion**: Adapter support for MoE models is not implemented
   - `forward_base()` works
   - `forward_lora()` will return an error for MoE models

2. **No Custom Routing**: Expert selection is entirely CoreML-managed
   - Cannot override routing decisions
   - No manual load balancing

3. **Requires config.json**: Models without valid config.json are treated as dense

### Future Work

- LoRA adapter fusion for MoE models
- Expert-specific adapter support
- Routing analytics and monitoring

## Testing

Run MoE configuration tests:

```bash
cargo test -p adapteros-lora-kernel-coreml moe
```

**Test Coverage**:
- MoE configuration parsing
- Validation logic
- `.mlpackage` directory structure handling
- Non-MoE model detection

## API Reference

### MoEConfig

```rust
pub struct MoEConfig {
    pub num_experts: usize,
    pub num_experts_per_tok: usize,
    pub hidden_size: usize,
    pub moe_intermediate_size: usize,
    pub num_shared_experts: Option<usize>,
}

impl MoEConfig {
    pub fn from_config_json(path: &Path) -> Result<Option<Self>>;
    pub fn is_moe(&self) -> bool;
    pub fn validate(&self) -> Result<()>;
    pub fn description(&self) -> String;
}
```

### CoreMLBackend Extensions

```rust
impl CoreMLBackend {
    pub fn is_moe_model(&self) -> bool;
    pub fn moe_config(&self) -> Option<&MoEConfig>;
}
```

## Architecture

```
┌─────────────────┐
│  Application    │
└────────┬────────┘
         │ FusedKernels trait
         ▼
┌─────────────────┐
│ CoreMLBackend   │
│  - moe_config   │◄─── Automatic detection on load_model()
└────────┬────────┘
         │ coreml_load_model()
         ▼
┌─────────────────┐
│ CoreML Runtime  │◄─── Native MoE expert routing
│  (.mlpackage)   │     and inference on ANE
└─────────────────┘
```

## Examples

### Basic Usage

```rust
use adapteros_lora_kernel_coreml::*;

let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndNeuralEngine, false)?;
backend.load_model(Path::new("model.mlpackage"))?;

if backend.is_moe_model() {
    println!("Running MoE inference on ANE");
}
```

### With FusedKernels Trait

```rust
use adapteros_lora_kernel_api::FusedKernels;

let mut backend = create_backend()?; // CoreMLBackend
let logits = backend.forward_base(&input_ids)?;
```

## Documentation

- **Implementation Guide**: `/docs/COREML_MOE_IMPLEMENTATION.md`
- **Design Document**: `/docs/COREML_MOE_DESIGN.md`
- **API Docs**: `cargo doc --open -p adapteros-lora-kernel-coreml`

## License

Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.
