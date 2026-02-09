# CoreML Conversion Quick Start

Fast-track guide to converting MLX MoE models to CoreML.

## TL;DR

```bash
# 1. Install dependencies
pip install -r scripts/requirements-convert.txt

# 2. Test conversion (single layer, ~5 min)
./scripts/test_coreml_conversion.sh

# 3. Full conversion (4 layers, 16 experts, ~30 min)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML.mlpackage \
  --seq-len 512
```

## What Gets Converted

### Current (MVP)
- **Layers**: First 4 of 48 transformer layers
- **Experts**: First 16 of 128 experts per layer
- **Sequence**: Configurable (default 512 tokens)
- **Precision**: FP16 (dequantized from 4-bit MLX)
- **Size**: ~8-10 GB for 4 layers

### Architecture Per Layer
```
Input (batch=1, seq=512, hidden=2048)
  ↓
LayerNorm (RMSNorm)
  ↓
Attention (32 heads, GQA with 4 KV heads)
  ↓
Residual Add
  ↓
LayerNorm (RMSNorm)
  ↓
MoE Block:
  - Router → top-8 expert selection
  - 16 Expert MLPs (gate/up/down projections)
  - Weighted combination
  ↓
Residual Add
  ↓
Output (batch=1, seq=512, hidden=2048)
```

## File Outputs

```
var/models/
├── Qwen3-30B-test-layer0.mlpackage/    # Test output (1 layer)
│   ├── Data/
│   │   └── weights/                     # Model weights
│   ├── Manifest.json                    # Package manifest
│   └── Metadata/
│       └── ...
└── Qwen3-30B-CoreML.mlpackage/         # Full output (4 layers)
    └── ... (same structure)
```

## Common Use Cases

### Fast Testing
```bash
# Minimal conversion for pipeline validation
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/test.mlpackage \
  --seq-len 128 \
  --single-layer 0
```
**Time**: 3-5 minutes
**Size**: ~400 MB

### Development
```bash
# Standard conversion for development/testing
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML-512.mlpackage \
  --seq-len 512
```
**Time**: 20-30 minutes
**Size**: ~8 GB

### Production (Future)
```bash
# Full model with all layers and experts
# NOTE: Not yet implemented - requires code changes
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML-FULL.mlpackage \
  --seq-len 2048 \
  --all-layers \
  --all-experts
```
**Time**: 2-4 hours (estimated)
**Size**: ~40-50 GB (estimated)

## Sequence Length Guide

| seq_len | Use Case | Memory | Speed |
|---------|----------|--------|-------|
| 128     | Testing  | Low    | Fast  |
| 512     | Development | Medium | Good |
| 1024    | Production | High | Medium |
| 2048    | Long context | Very High | Slow |

**Recommendation**: Start with 512 for good balance.

## Troubleshooting

### "ModuleNotFoundError: No module named 'coremltools'"
```bash
pip install coremltools>=9.0.0
```

### "ModuleNotFoundError: No module named 'coreml_moe_ops'"
```bash
# Run from project root
cd /path/to/adapter-os
python scripts/convert_mlx_to_coreml.py ...
```

### "Process killed" (OOM)
- Reduce seq_len: `--seq-len 128`
- Use single layer: `--single-layer 0`
- Close other apps
- Use Mac with 32GB+ RAM

### "Conversion taking too long"
- Normal for large models
- Use `--single-layer 0` for testing
- Be patient (20-30 min is expected)

## Next Steps After Conversion

1. **Verify Output**
   ```bash
   ls -lh ./var/models/Qwen3-30B-CoreML.mlpackage/
   ```

2. **Inspect Model**
   ```python
   import coremltools as ct
   model = ct.models.MLModel("./var/models/Qwen3-30B-CoreML.mlpackage")
   print(model)
   print(model.user_defined_metadata)
   ```

3. **Implement CoreML Backend** (Phase 3)
   - Create Rust FFI bindings
   - Implement inference engine
   - Add to aos-worker

4. **Benchmark Performance**
   - Measure tokens/sec
   - Check ANE utilization
   - Compare vs MLX baseline

## Files Reference

| File | Purpose |
|------|---------|
| `convert_mlx_to_coreml.py` | Main conversion script |
| `coreml_moe_ops.py` | MoE utilities and ops |
| `requirements-convert.txt` | Python dependencies |
| `test_coreml_conversion.sh` | Automated test script |
| `COREML_CONVERSION.md` | Full documentation |
| `manifests/qwen3-30b-coreml.yaml` | Model manifest template |

## Performance Expectations

### Conversion Performance
- **Single layer**: 3-5 minutes
- **4 layers (MVP)**: 20-30 minutes
- **Full model**: 2-4 hours (estimated)

### Inference Performance (estimated, ANE)
- **Throughput**: 5-20 tokens/sec
- **Latency**: 50-200ms per token
- **Memory**: 10-15 GB

*Actual performance depends on hardware and optimization.*

## Advanced Options

### Custom Expert Count
Edit `scripts/convert_mlx_to_coreml.py`:
```python
# Line ~285
max_experts_to_load = min(num_experts, 16)  # Change 16 to desired count
```

### Custom Layer Count
Edit `scripts/convert_mlx_to_coreml.py`:
```python
# Line ~335
num_layers_to_build = min(num_layers, 4)  # Change 4 to desired count
```

### Full Model (All Layers, All Experts)
```python
# Line ~285 - Remove expert limit
max_experts_to_load = num_experts  # Load all 128 experts

# Line ~335 - Remove layer limit
num_layers_to_build = num_layers  # Build all 48 layers
```

**Warning**: This will require 32GB+ RAM and take hours.

## FAQ

**Q: Why only 4 layers?**
A: Memory management. Full 48 layers with 128 experts requires 20GB+ RAM during conversion.

**Q: Why only 16 experts?**
A: Same reason. Each expert has 3 weight matrices. 128 × 3 × 48 layers = huge memory footprint.

**Q: Can I convert the full model?**
A: Yes, but you'll need to modify the hardcoded limits in the script and have sufficient RAM.

**Q: Why FP16 instead of keeping 4-bit?**
A: CoreML doesn't have full support for 4-bit quantization. We dequantize to FP16 which ANE handles well.

**Q: How do I use the converted model?**
A: Phase 3 will implement the CoreML backend in Rust. For now, it's a standalone .mlpackage.

**Q: Can I convert other MoE models?**
A: The script is designed for Qwen3 architecture. Other MoE models may need modifications.

## Support

For issues:
1. Check `COREML_CONVERSION.md` for detailed docs
2. Run test script: `./scripts/test_coreml_conversion.sh`
3. Check logs for specific errors
4. Verify all dependencies installed
