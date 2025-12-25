# CoreML MoE Backend Implementation (Phase 3)

This document describes the Phase 3 implementation of CoreML Mixture of Experts (MoE) support in AdapterOS.

## Overview

Phase 3 extends the CoreML kernel backend to natively support MoE models (like Qwen3-30B-MoE) that have been converted to CoreML `.mlpackage` format. This enables efficient inference of MoE architectures on Apple Neural Engine (ANE).

## Architecture

### Implementation Strategy

The CoreML MoE implementation follows a **zero-overhead detection** approach:

1. **Automatic Detection**: MoE architecture is detected from the model's `config.json` during model loading
2. **Native CoreML Runtime**: Expert routing and inference are handled entirely by the CoreML runtime - no custom routing logic needed
3. **Transparent Integration**: The existing `forward_base()` path works seamlessly with MoE models
4. **No LoRA (Initially)**: Phase 3 focuses on base model inference; LoRA fusion for MoE is future work

### Key Components

#### 1. MoE Configuration Module (`moe.rs`)

**Location**: `crates/adapteros-lora-kernel-coreml/src/moe.rs`

Provides configuration parsing and validation for MoE models:

```rust
pub struct MoEConfig {
    pub num_experts: usize,              // Total expert count
    pub num_experts_per_tok: usize,      // Top-k routing
    pub hidden_size: usize,              // Hidden dimension
    pub moe_intermediate_size: usize,    // Expert FFN size
    pub num_shared_experts: Option<usize>, // Shared experts (optional)
}
```

**Features**:
- Loads from `config.json` (supports multiple locations: model root, `.mlpackage/Data/com.apple.CoreML`, parent directory)
- Validates MoE parameters for consistency
- Provides human-readable architecture descriptions

**Example Detection**:
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

#### 2. CoreMLBackend Extensions

**Location**: `crates/adapteros-lora-kernel-coreml/src/lib.rs`

Extended `CoreMLBackend` struct with MoE support:

```rust
pub struct CoreMLBackend {
    // ... existing fields
    moe_config: Option<MoEConfig>,  // Detected MoE configuration
}
```

**New Methods**:
- `is_moe_model() -> bool` - Check if loaded model is MoE
- `moe_config() -> Option<&MoEConfig>` - Get MoE configuration

**Detection Flow**:
1. Model loads via `load_model(model_path)`
2. After successful CoreML load, attempt `MoEConfig::from_config_json(model_path)`
3. If MoE detected, validate configuration and log architecture details
4. Store configuration in `moe_config` field

**Logging**:
```
INFO Detected MoE model architecture moe_desc="MoE: 60 experts + 8 shared, top-8 routing, hidden=3584, intermediate=1408"
INFO Loaded CoreML model is_moe=true
```

#### 3. CoreML Bridge (ObjC++)

**Location**: `crates/adapteros-lora-kernel-coreml/src/coreml_bridge.mm`

**Key Insight**: No changes needed! The `coreml_load_model()` function already handles both `.mlmodelc` and `.mlpackage` formats transparently:

```objc
// CoreML automatically handles both .mlmodelc and .mlpackage formats
// For MoE models converted to .mlpackage, the expert routing and inference
// are handled natively by the CoreML runtime (no special handling needed here)
MLModel *model = [MLModel modelWithContentsOfURL:modelURL
                                   configuration:config
                                           error:&error];
```

The CoreML runtime:
- Automatically detects MoE operations in the model graph
- Routes tokens to the appropriate experts during inference
- Performs top-k expert selection natively
- Executes expert FFNs in parallel where possible

#### 4. Backend Factory Integration

**Location**: `crates/adapteros-lora-worker/src/backend_factory.rs`

The factory already supports MoE models through the existing `create_coreml_backend()` function. MoE detection happens automatically when the backend loads the model.

**Usage Note**:
```rust
// Note: MoE detection happens automatically in backend.load_model()
// The backend will detect and log MoE architecture from config.json
```

## Usage

### Converting MLX MoE Model to CoreML

Use the Phase 2 conversion pipeline:

```bash
# Convert Qwen3-30B-MoE from MLX to CoreML
python scripts/convert_mlx_to_coreml.py \
  --model-path ./var/models/Qwen3-30B-Instruct-MLX \
  --output-path ./var/models/Qwen3-30B-CoreML.mlpackage \
  --compute-units ALL \
  --quantize none
```

### Running Inference with CoreML MoE Backend

```bash
# Build with CoreML support
cargo build --release -p adapteros-lora-worker --features coreml-backend,multi-backend

# Start worker with CoreML backend and MoE model
./target/release/aos-worker \
  --backend coreml \
  --model-path ./var/models/Qwen3-30B-CoreML.mlpackage \
  --tenant-id tenant-test \
  --plan-id dev \
  --uds-path ./var/run/worker.sock

# Test inference (base model only, no adapters)
curl -X POST http://localhost:8080/api/v1/infer/stream \
  -H "Authorization: Bearer $(cat ./var/token.txt)" \
  -d '{"prompt": "def hello():", "max_tokens": 50, "adapters": []}'
```

### Expected Logs

```
INFO Initializing CoreML backend compute_units=CpuAndNeuralEngine
INFO Detected MoE model architecture moe_desc="MoE: 60 experts + 8 shared, top-8 routing, hidden=3584, intermediate=1408"
INFO Loaded CoreML model is_moe=true hash=a1b2c3d4e5f6 compiled_path=/var/models/Qwen3-30B-CoreML.mlpackage
INFO CoreML backend ready device="Apple M4 Max" ane_available=true
```

## Implementation Details

### Model Detection Logic

The MoE detection prioritizes different field names to support various model formats:

```rust
// Check multiple field names for compatibility
let num_experts = config
    .get("num_experts")
    .or_else(|| config.get("num_local_experts"))  // Mixtral format
    .and_then(|v| v.as_u64())
    .map(|v| v as usize);

let num_experts_per_tok = config
    .get("num_experts_per_tok")
    .or_else(|| config.get("num_experts_per_token"))
    .or_else(|| config.get("top_k"))  // Alternative naming
    .and_then(|v| v.as_u64())
    .map(|v| v as usize)
    .unwrap_or(2);  // Default to top-2 routing
```

### Validation

MoE configurations are validated for consistency:

```rust
pub fn validate(&self) -> Result<()> {
    if self.num_experts == 0 {
        return Err(AosError::Config("MoE num_experts must be greater than 0".to_string()));
    }

    if self.num_experts_per_tok > self.num_experts {
        return Err(AosError::Config(format!(
            "MoE num_experts_per_tok ({}) cannot exceed num_experts ({})",
            self.num_experts_per_tok, self.num_experts
        )));
    }

    // ... additional validation
    Ok(())
}
```

### Error Handling

MoE detection failures are non-fatal:

```rust
match MoEConfig::from_config_json(model_path) {
    Ok(Some(config)) => {
        // Validate and use MoE config
    }
    Ok(None) => {
        // Not an MoE model - normal dense architecture
        self.moe_config = None;
    }
    Err(e) => {
        // Failed to load config.json - log but don't fail
        tracing::debug!("Could not load model config.json for MoE detection: {}", e);
        self.moe_config = None;
    }
}
```

## Performance Characteristics

### Apple Neural Engine (ANE) Acceleration

MoE models converted to CoreML can leverage ANE for:

1. **Expert Routing**: Gating network computations run on ANE
2. **Expert FFNs**: Matrix multiplications in expert networks are ANE-optimized
3. **Shared Experts**: Efficiently processes shared expert computations

### Compute Unit Selection

For optimal MoE performance, use:

```rust
ComputeUnits::CpuAndNeuralEngine  // Recommended for deterministic, power-efficient inference
// or
ComputeUnits::All  // Maximum performance (may use GPU for some ops)
```

### Memory Efficiency

MoE models in CoreML:
- Load only the required expert weights on-demand (CoreML handles this automatically)
- Benefit from memory mapping of `.mlpackage` format
- Can use quantization to reduce memory footprint

## Limitations (Phase 3)

### **CRITICAL LIMITATION: No LoRA Adapter Support**

**⚠️ MoE models DO NOT support LoRA adapter fusion in the current implementation.**

- ✅ **Base model inference WORKS**: You can run MoE models without adapters
- ❌ **Adapter fusion FAILS**: Any attempt to load, attach, or switch adapters will return an error
- ❌ **No adapter hot-swapping**: Cannot use adapters with MoE models at all

**Error Behavior**: When attempting to use adapters with an MoE model, the following methods will return `AosError::Kernel`:
- `load_adapter()` - Cannot load adapters for MoE models
- `attach_adapter()` - Cannot attach adapters for MoE models
- `switch_adapter()` - Cannot switch adapters for MoE models

**Error Message**:
```
LoRA adapter fusion is not supported for MoE models. MoE base inference works, but adapter fusion requires additional implementation. See docs/COREML_MOE_IMPLEMENTATION.md for details.
```

**Why This Limitation Exists**:
- MoE architectures have multiple expert networks per layer
- LoRA fusion requires applying adapters to each expert independently
- Current implementation only supports single dense FFN layers
- Implementing MoE + LoRA fusion requires:
  1. Expert-specific adapter application
  2. Routing-aware fusion (weight by expert selection)
  3. Shared expert handling
  4. CoreML graph modifications for fused MoE+LoRA inference

**Workaround**:
- Use MoE models for base inference only (no adapters)
- Use dense models (non-MoE) if adapter support is required
- Future Phase 4+ will implement MoE + LoRA fusion

### Other Limitations

2. **No Custom Routing**: Expert routing is entirely handled by CoreML runtime
   - Cannot override expert selection
   - No manual load balancing

3. **Config.json Required**: MoE detection requires a valid `config.json` file
   - Models without config.json are treated as dense models
   - No fallback architecture detection

## Testing

### Unit Tests

Run MoE configuration tests:

```bash
cargo test -p adapteros-lora-kernel-coreml --lib moe
```

**Test Coverage**:
- MoE configuration parsing from valid JSON
- Non-MoE model detection
- Invalid configuration rejection
- `.mlpackage` directory structure handling
- Configuration validation logic

### Integration Tests

```bash
# Test with actual .mlpackage model (if available)
cargo test -p adapteros-lora-kernel-coreml --lib --features coreml-backend -- --nocapture
```

## Future Work (Phase 4+)

### LoRA Fusion for MoE

To support LoRA adapters with MoE models:

1. **Expert-Specific Adapters**: Apply LoRA to individual experts
2. **Routing-Aware Fusion**: Weight adapter contributions by expert selection
3. **Shared Expert Adapters**: Apply adapters to shared expert layers

### Advanced Routing

Potential enhancements:

1. **Load Balancing**: Monitor expert utilization and optimize routing
2. **Expert Caching**: Cache frequently-used experts in ANE memory
3. **Batch Routing**: Optimize routing decisions across batch dimensions

### Multi-Model Support

Extend to other MoE architectures:

- Mixtral-8x7B
- DeepSeek-MoE
- Custom MoE architectures

## Architecture Decisions

### Why No Custom Routing in Phase 3?

**Rationale**: The CoreML runtime provides highly optimized, ANE-accelerated expert routing that outperforms any custom CPU/Metal implementation:

- **ANE Optimization**: CoreML compiles routing networks to ANE-specific ops
- **Zero Overhead**: No IPC or data copying between runtime and custom code
- **Maintenance**: Leverages Apple's optimizations automatically

### Why Detect from config.json?

**Rationale**: The model architecture is metadata that doesn't change:

- **Deterministic**: Same model always has same architecture
- **Authoritative**: config.json is the source of truth
- **Portable**: Works across different model formats (MLX, HF, CoreML)

## References

- **Phase 1**: MLX subprocess bridge (`docs/COREML_MOE_DESIGN.md`)
- **Phase 2**: Model conversion pipeline (`scripts/convert_mlx_to_coreml.py`)
- **Phase 3**: This implementation (CoreML native MoE)

## Contributing

When extending MoE support:

1. **Add tests** to `crates/adapteros-lora-kernel-coreml/src/moe.rs`
2. **Update validation** in `MoEConfig::validate()`
3. **Document** any new MoE parameters or architectures
4. **Log** detection results for debugging

## License

Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.
