# CoreML Export Scripts

These Python scripts require `coremltools` which has no Rust equivalent.
They are the **only** Python scripts that should remain in the training pipeline.

## Requirements

```bash
pip install coremltools torch transformers mlx mlx-lm
```

## Scripts

- `export_coreml_model.py` - Export base models to CoreML `.mlpackage`
- `export_coreml_production.py` - Production CoreML export with FP16 optimization
- `convert_to_coreml.py` - Convert MLX LoRA adapters to CoreML

## Why Python?

Apple's `coremltools` library is Python-only. There is no Rust crate that can:

1. Convert PyTorch/MLX models to CoreML format
2. Optimize models for Apple Neural Engine (ANE)
3. Generate `.mlpackage` archives

All other training operations (training loop, backward pass, optimizer) are
implemented in Rust via MLX FFI bindings.
