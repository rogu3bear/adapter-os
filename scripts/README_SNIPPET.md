# CoreML MoE Model Conversion

Convert MLX MoE (Mixture-of-Experts) models to CoreML `.mlpackage` format for Apple Neural Engine execution.

## Quick Start

```bash
# 1. Install dependencies
pip install -r scripts/requirements-convert.txt

# 2. Inspect model
python scripts/inspect_mlx_model.py ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit

# 3. Test conversion (single layer, ~5 min)
./scripts/test_coreml_conversion.sh

# 4. Convert model (4 layers, 16 experts, ~30 min)
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \
  --output ./var/models/Qwen3-30B-CoreML.mlpackage \
  --seq-len 512
```

## Features

- **MoE Architecture Support**: Handles 128 experts with top-k routing
- **Quantization**: Dequantizes 4-bit MLX weights to FP16
- **ANE Optimized**: Exports for Apple Neural Engine execution
- **Flexible**: Configurable sequence length and layer subset
- **Memory Efficient**: MVP mode for testing on 16GB+ Macs

## Documentation

- **[QUICKSTART_CONVERSION.md](scripts/QUICKSTART_CONVERSION.md)** - Fast-track guide
- **[COREML_CONVERSION.md](scripts/COREML_CONVERSION.md)** - Comprehensive documentation
- **[PHASE2_SUMMARY.md](scripts/PHASE2_SUMMARY.md)** - Implementation summary

## Scripts

| Script | Purpose |
|--------|---------|
| `convert_mlx_to_coreml.py` | Main conversion pipeline |
| `coreml_moe_ops.py` | MoE operations and utilities |
| `inspect_mlx_model.py` | Model inspection and estimates |
| `test_coreml_conversion.sh` | Automated testing |

## Requirements

- macOS 14+ (for CoreML support)
- Python 3.9+
- coremltools 9.0+
- 16GB+ RAM (32GB recommended for full conversion)

## Next Steps

After conversion:
1. Implement CoreML backend (Phase 3)
2. Integrate with aos-worker
3. Benchmark on Apple Silicon
