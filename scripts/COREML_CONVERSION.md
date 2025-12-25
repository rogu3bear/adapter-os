# CoreML Model Conversion & LoRA Fusion Guide

This guide explains how to convert models to CoreML `.mlpackage` format and integrate LoRA adapters for Apple Neural Engine (ANE) execution.

## Overview

AdapterOS supports two CoreML workflows:

1. **Base Model Conversion**: Convert MLX/safetensors models to CoreML (this guide)
2. **LoRA Adapter Integration**: Two paths available:
   - **Offline Pre-Fusion** (recommended): Fuse LoRA weights before CoreML conversion
   - **Runtime Sidecar** (future): Hot-swap adapters at runtime (currently stubbed)

## LoRA Integration Workflows

### Option A: Offline Pre-Fusion (Recommended)

**Best for:** Production deployments with known adapter combinations

**Steps:**
1. Convert base model to safetensors (if not already in that format)
2. Use `fusion` module to pre-fuse LoRA weights into base model weights
3. Convert fused weights to CoreML `.mlpackage`
4. Deploy the pre-fused package

**Advantages:**
- ✅ Zero runtime overhead
- ✅ Full ANE optimization
- ✅ Deterministic audit trails

**Example:**

```rust
use adapteros_lora_kernel_coreml::fusion::{
    LoraFusionConfig, AdapterFusionSpec, fuse_lora_into_model
};

// Step 1: Fuse LoRA weights into base model
let config = LoraFusionConfig {
    base_model_path: "base_weights.safetensors".into(),
    output_path: "fused_weights.safetensors".into(),
    adapters: vec![AdapterFusionSpec {
        weights_path: "adapter.safetensors".into(),
        gate_weight: 1.0,
        alpha: 32.0,
        rank: 16,
    }],
    compute_units: ComputeUnits::CpuAndNeuralEngine,
};

let result = fuse_lora_into_model(&config)?;
println!("Fused {} layers", result.layers_fused);

// Step 2: Convert fused weights to CoreML (use scripts/convert_mlx_to_coreml.py)
```

See `crates/adapteros-lora-kernel-coreml/README.md` for detailed fusion API documentation.

### Option B: Runtime Sidecar (Future Work)

**Status:** ⚠️ **STUBBED** - Infrastructure exists but LoRA computation not implemented

This would enable hot-swapping adapters at runtime via Metal/MLX sidecar, but adds ~20-30% overhead. Use offline pre-fusion for production.

---

## Base Model Conversion (MoE Models)

The conversion pipeline transforms MLX safetensors models (including MoE architecture) into CoreML format optimized for ANE.

**Target Model**: Qwen3-Coder-30B-A3B-Instruct-MLX-4bit
- 128 experts
- 8 experts per token (top-k routing)
- 48 transformer layers
- 2048 hidden size
- 768 MoE intermediate size
- 4-bit quantization (group_size=64)

## Prerequisites

### 1. Install Dependencies

```bash
# Create a virtual environment (recommended)
python3 -m venv .venv-convert
source .venv-convert/bin/activate

# Install requirements
pip install -r scripts/requirements-convert.txt
```

### 2. Download MLX Model

Ensure you have the MLX model downloaded:

```bash
# Model should be at:
ls -lh ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit/

# Should contain:
# - config.json
# - model-00001-of-00004.safetensors
# - model-00002-of-00004.safetensors
# - model-00003-of-00004.safetensors
# - model-00004-of-00004.safetensors
# - model.safetensors.index.json
# - tokenizer files
```

## Conversion Process

### Quick Start (Single Layer Test)

For initial testing, convert a single layer:

```bash
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-layer0.mlpackage \
  --seq-len 512 \
  --single-layer 0
```

This will:
- Load weights for layer 0 only
- Dequantize from 4-bit to FP16
- Build CoreML MIL graph
- Export to `.mlpackage`

**Expected time**: 5-10 minutes
**Expected size**: ~500 MB

### Multi-Layer Conversion

Convert multiple layers with all 128 experts (recommended for 32GB+ RAM):

```bash
# 8 layers with all 128 experts (safe for 32GB MacBook Pro)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-Coder-8layer-CoreML.mlpackage \
  --num-layers 8 \
  --num-experts 128 \
  --seq-len 512

# 12 layers (pushes 32GB limit, may use swap)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-Coder-12layer-CoreML.mlpackage \
  --num-layers 12 \
  --num-experts 128 \
  --seq-len 512
```

**Tested scaling** (all 128 experts):
| Layers | Peak RAM | Output Size | Time |
|--------|----------|-------------|------|
| 2      | 11 GB    | 3.5 GB      | 40s  |
| 4      | 19 GB    | 5.8 GB      | 67s  |
| 8      | 20 GB    | 10.5 GB     | 2.5 min |
| 12     | 27 GB    | 15 GB       | 4.3 min |

**Recommendation**: Use `--num-layers 8` for safe conversion on 32GB systems.

### Sequence Length Variants

Create models for different sequence lengths:

```bash
# Short sequences (faster inference)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML-512.mlpackage \
  --seq-len 512

# Medium sequences
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML-1024.mlpackage \
  --seq-len 1024

# Long sequences (slower but more context)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML-2048.mlpackage \
  --seq-len 2048
```

**Tip**: Use multiples of 8 for optimal ANE performance (512, 1024, 2048, 4096).

## Architecture Details

### MoE Layer Structure

Each MoE layer contains:

1. **Router Network**: Projects hidden states to expert logits
   ```python
   router_logits = linear(hidden_states, router_weight)
   # Shape: (batch, seq_len, num_experts=128)
   ```

2. **Top-K Selection**: Selects top 8 experts per token
   ```python
   routing_weights, selected_experts = topk(softmax(router_logits), k=8)
   ```

3. **Expert MLPs**: Each expert has 3 projections
   ```python
   # For each expert:
   gate_out = silu(linear(hidden_states, gate_proj))
   up_out = linear(hidden_states, up_proj)
   mlp_out = linear(gate_out * up_out, down_proj)
   ```

4. **Weighted Combination**: Combines expert outputs by routing weights
   ```python
   output = sum(routing_weight[i] * expert_output[i] for i in top_k_experts)
   ```

### Quantization Handling

MLX 4-bit quantization uses grouped quantization:

- **Packed Format**: 2 values per byte (4-bit each)
- **Group Size**: 64 consecutive values share scale/bias
- **Dequantization**: `value = (quantized_value * scale) + bias`

The conversion script automatically dequantizes to FP16 for CoreML:

```python
dequantized = loader.dequantize_weight(
    weight, scales, biases,
    group_size=64,
    bits=4
)
```

### CoreML Optimizations

- **Compute Precision**: FP16 for ANE optimization
- **Target**: macOS 14+ for latest CoreML features
- **Compute Units**: ALL (CPU + GPU + ANE)
- **Format**: MLProgram (modern CoreML format)

## Testing

### 1. Verify Conversion

Check that the `.mlpackage` was created:

```bash
ls -lh ./var/models/Qwen3-30B-layer0.mlpackage/

# Should contain:
# - Data/
# - Manifest.json
# - Metadata/
```

### 2. Inspect Model

Use CoreML tools to inspect:

```python
import coremltools as ct

model = ct.models.MLModel("./var/models/Qwen3-30B-layer0.mlpackage")
print(model)
print(model.user_defined_metadata)
```

### 3. Test Inference (Future)

Once the CoreML backend is implemented:

```bash
./target/release/aos-worker \
  --backend coreml \
  --model-path ./var/models/Qwen3-30B-layer0.mlpackage
```

## Troubleshooting

### Memory Issues

**Problem**: Python process killed due to OOM

**Solutions**:
- Start with `--single-layer` mode
- Reduce sequence length (`--seq-len 128`)
- Close other applications
- Use a machine with more RAM (32GB+ recommended)

### Slow Conversion

**Problem**: Conversion taking hours

**Expected**: Full model conversion is slow due to:
- 128 experts × 48 layers = 6,144 expert networks
- Dequantization of 4-bit weights
- CoreML graph optimization

**Solutions**:
- Use `--single-layer` for testing
- The script already limits to 16 experts and 4 layers for MVP
- Run on Apple Silicon Mac for faster performance

### Import Errors

**Problem**: `ModuleNotFoundError: No module named 'coremltools'`

**Solution**:
```bash
pip install coremltools>=9.0.0
```

**Problem**: `ModuleNotFoundError: No module named 'coreml_moe_ops'`

**Solution**: Run from project root:
```bash
cd /Users/mln-dev/Dev/adapter-os
python scripts/convert_mlx_to_coreml.py ...
```

Or add to PYTHONPATH:
```bash
export PYTHONPATH="${PYTHONPATH}:/Users/mln-dev/Dev/adapter-os/scripts"
```

### ANE Warnings

**Warning**: `seq_len=1000 is not a multiple of 8 (ANE optimization)`

**Impact**: Slight performance degradation on ANE

**Fix**: Use multiples of 8: 512, 1024, 2048, 4096, etc.

## File Structure

```
scripts/
├── convert_mlx_to_coreml.py    # Main conversion script
├── coreml_moe_ops.py            # MoE operations and utilities
├── requirements-convert.txt     # Python dependencies
└── COREML_CONVERSION.md         # This guide

manifests/
└── qwen3-30b-coreml.yaml        # Model manifest template
```

## Implementation Notes

### Multi-Layer Scaling Analysis (Tested December 2024)

Comprehensive testing with **all 128 experts** on 32GB MacBook Pro:

#### Conversion Memory & Time Scaling

| Layers | Peak RAM | Output Size | Conversion Time |
|--------|----------|-------------|-----------------|
| 2      | 11.0 GB  | 3.48 GB     | 40s             |
| 4      | 19.2 GB  | 5.80 GB     | 67s             |
| 8      | 20.3 GB  | 10.45 GB    | 152s (2.5 min)  |
| 12     | 27.3 GB  | 15.09 GB    | 262s (4.3 min)  |

**Key Finding**: Peak RAM doesn't scale linearly due to batched weight loading with GC. The batching strategy (16 experts per batch) effectively manages memory.

#### Inference Performance (512-token sequence, CPU+ANE)

| Layers | Size   | Cold (ms) | P50 (ms) | ms/layer |
|--------|--------|-----------|----------|----------|
| 2      | 3.7 GB | 723       | 537      | 269      |
| 4      | 6.2 GB | 1245      | 943      | 236      |
| 8      | 11.2 GB| 2389      | 1764     | 221      |

**Inference scales linearly at ~220ms per layer** (warm, 512 tokens).

#### Recommendations by RAM

| Available RAM | Max Layers | Full 128 Experts |
|---------------|------------|------------------|
| 16 GB         | 2-4        | ⚠️ Tight         |
| 32 GB         | 8-12       | ✅ Yes           |
| 64 GB         | 24+        | ✅ Yes           |
| 128 GB        | 48 (full)  | ✅ Yes           |

**For 32GB MacBook Pro**: Use `--num-layers 8` for safe conversion with all experts.

#### Extrapolation to Full Model (48 layers)

- **Estimated output size**: ~56 GB
- **Estimated conversion RAM**: ~85-100 GB
- **Estimated inference time**: ~10.6 seconds per 512-token forward pass

**Recommendation**: For 32GB systems, convert in chunks (8-12 layers each) and implement layer-wise model loading in the Rust backend.

### Current Limitations (MVP)

1. **Layer Subset**: Converts configurable subset of 48 layers
   - Use `--num-layers N` to control (tested up to 12 on 32GB)
   - 8 layers is the sweet spot for 32GB RAM

2. **Expert Support**: Full 128 experts now work within 32GB
   - Use `--num-experts 128` for full expert coverage
   - Batched loading (16 at a time) manages memory effectively

3. **Dense Expert Computation**: All loaded experts are computed
   - Simpler than sparse dispatch
   - Less efficient but easier to implement in CoreML
   - Future: implement sparse gather/scatter for selected experts only

4. **Simplified Attention**: Basic scaled dot-product attention
   - Full implementation needs proper multi-head reshaping
   - Missing: RoPE positional embeddings
   - Missing: KV caching for autoregressive generation

### Future Improvements

1. **Full Model Support**
   - Remove layer/expert limits
   - Implement layer-by-layer conversion to manage memory
   - Support for full 128 experts with sparse dispatch

2. **Sparse Expert Dispatch**
   - Use CoreML `gather`/`scatter` for true sparse MoE
   - Only compute selected top-k experts
   - Significant performance improvement

3. **Complete Attention Implementation**
   - Multi-head attention with proper reshaping
   - RoPE positional embeddings
   - KV caching for generation

4. **Quantization Preservation**
   - Keep 4-bit quantization in CoreML (if supported)
   - Currently dequantizes to FP16 (larger model)

5. **Validation Pipeline**
   - Compare CoreML output vs. MLX reference
   - Automated testing of conversion accuracy

## Next Steps

1. **Test Conversion**: Run single-layer conversion
2. **Implement CoreML Backend**: Create Rust backend for inference
3. **Benchmark Performance**: Measure ANE utilization and throughput
4. **Expand Coverage**: Gradually increase layers and experts
5. **Optimize**: Implement sparse dispatch and quantization

## References

- [CoreML Tools Documentation](https://apple.github.io/coremltools/)
- [CoreML MIL Operations](https://apple.github.io/coremltools/source/coremltools.converters.mil.mil.ops.defs.html)
- [Qwen3 MoE Architecture](https://huggingface.co/Qwen/Qwen3-Coder-30B)
- [MLX Quantization](https://ml-explore.github.io/mlx/build/html/usage/quantization.html)
