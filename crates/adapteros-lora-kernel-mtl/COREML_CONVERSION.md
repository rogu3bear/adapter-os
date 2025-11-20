# CoreML Model Conversion Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-01-19

This guide covers the complete workflow for converting models from safetensors to CoreML format with ANE optimization.

---

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Conversion Workflow](#conversion-workflow)
4. [Quantization Strategies](#quantization-strategies)
5. [Weight Mapping](#weight-mapping)
6. [Validation](#validation)
7. [Performance Optimization](#performance-optimization)
8. [Troubleshooting](#troubleshooting)

---

## Overview

The CoreML conversion utilities enable automated conversion of transformer models from safetensors to CoreML `.mlpackage` format with:

- **Multi-architecture support**: Qwen2.5, LLaMA, Mistral, GPT-NeoX
- **Quantization**: FP16, INT8, INT4 with ANE compatibility
- **Automated weight mapping**: Transformer layers → CoreML operations
- **Validation**: Numerical accuracy and ANE compatibility checks
- **Performance benchmarking**: Throughput and latency measurements

### Key Features

| Feature | Description | Status |
|---------|-------------|--------|
| Safetensors → CoreML | Convert weights to CoreML format | ✅ Complete |
| FP16 Quantization | ANE-optimized 16-bit floating point | ✅ Complete |
| INT8 Quantization | 4x compression with calibration | ✅ Complete |
| INT4 Quantization | 8x compression (experimental) | ⚠️ Experimental |
| Weight Mapping | Auto-detect architecture and map layers | ✅ Complete |
| ANE Validation | Verify Neural Engine compatibility | ✅ Complete |
| Accuracy Validation | Compare outputs with original model | ✅ Complete |
| Performance Benchmarks | Measure throughput and latency | ✅ Complete |

---

## Quick Start

### Prerequisites

**Rust dependencies** (automatically handled by Cargo):
```toml
[dependencies]
adapteros-lora-kernel-mtl = { path = "..." }
```

**Python dependencies** (for conversion script):
```bash
pip install coremltools torch safetensors transformers
```

### Basic Conversion

```rust
use adapteros_lora_kernel_mtl::{
    ConversionConfig, ModelConverter, QuantizationType,
};
use std::path::Path;

// Configure conversion
let config = ConversionConfig {
    quantization: Some(QuantizationType::Float16), // FP16 for ANE
    target_ane: true,
    batch_size: 1,
    sequence_length: 128,
    min_macos_version: "13.0".to_string(),
    strict_validation: true,
};

// Create converter
let converter = ModelConverter::new(config)?;

// Convert model (generates Python script)
let manifest = converter.convert_safetensors_to_coreml(
    Path::new("model.safetensors"),
    Path::new("model.mlpackage"),
)?;

// Run generated Python script
println!("Run: python3 {}", manifest.script_path.display());
```

### Run Conversion Script

```bash
# Execute generated Python script
python3 model.mlpackage.conversion.py

# Output: model.mlpackage (CoreML model)
```

### Validate Conversion

```rust
use adapteros_lora_kernel_mtl::{ModelValidator, ValidationConfig};

let validator = ModelValidator::new(ValidationConfig::default());
let report = validator.validate_model(
    Path::new("model.safetensors"),
    Path::new("model.mlpackage"),
)?;

if report.passed() {
    println!("✅ Validation passed!");
} else {
    println!("❌ Validation failed: {:?}", report.errors);
}
```

---

## Conversion Workflow

### Step 1: Configure Conversion

```rust
use adapteros_lora_kernel_mtl::{ConversionConfig, QuantizationType};

let config = ConversionConfig {
    // Quantization (FP16 recommended for ANE)
    quantization: Some(QuantizationType::Float16),

    // Target Apple Neural Engine
    target_ane: true,

    // Batch size (ANE optimized for 1)
    batch_size: 1,

    // Maximum sequence length
    sequence_length: 128,

    // Minimum macOS version (13.0 for ANE)
    min_macos_version: "13.0".to_string(),

    // Enable strict validation
    strict_validation: true,
};
```

### Step 2: Create Converter

```rust
use adapteros_lora_kernel_mtl::{ModelConverter, ArchitectureType, LayerMapping};

let converter = ModelConverter::new(config)?;

// Optional: Set custom layer mapping
let converter = converter.with_layer_mapping(LayerMapping::qwen2_5());
```

### Step 3: Convert Model

```rust
let manifest = converter.convert_safetensors_to_coreml(
    Path::new("path/to/model.safetensors"),
    Path::new("path/to/model.mlpackage"),
)?;

// Manifest contains:
// - metadata: Model info (vocab size, hidden size, layers)
// - script_path: Generated Python conversion script
// - output_path: Target CoreML model path
```

### Step 4: Execute Python Script

The generated Python script performs the actual conversion:

```bash
python3 model.mlpackage.conversion.py
```

This script:
1. Loads safetensors weights
2. Infers model configuration
3. Creates PyTorch model architecture
4. Traces model with TorchScript
5. Converts to CoreML with quantization
6. Validates ANE compatibility
7. Saves `.mlpackage` + metadata JSON

### Step 5: Validate Conversion

```rust
use adapteros_lora_kernel_mtl::{ModelValidator, ValidationConfig};

let config = ValidationConfig {
    accuracy_threshold: 1e-3,       // 0.1% relative error
    num_samples: 10,                // Validation samples
    check_ane_compatibility: true,  // Verify ANE ops
    run_benchmarks: true,           // Measure performance
    warmup_iterations: 10,
    benchmark_iterations: 100,
};

let validator = ModelValidator::new(config);
let report = validator.validate_model(
    Path::new("original.safetensors"),
    Path::new("converted.mlpackage"),
)?;

// Check results
if let Some(accuracy) = &report.accuracy {
    println!("Mean relative error: {:.6}", accuracy.mean_relative_error);
}

if let Some(ane) = &report.ane_compatibility {
    println!("ANE compatible: {}", ane.fully_compatible);
}

if let Some(perf) = &report.performance {
    println!("Throughput: {:.1} tokens/sec", perf.throughput_tokens_per_sec);
}
```

---

## Quantization Strategies

### FP16 Quantization (Recommended)

**Best for:** Production deployments on ANE

```rust
let config = ConversionConfig {
    quantization: Some(QuantizationType::Float16),
    ..Default::default()
};
```

**Benefits:**
- ✅ Full ANE support
- ✅ 2x memory reduction
- ✅ Minimal accuracy loss (<0.1%)
- ✅ No calibration required

**Use cases:**
- Production inference on M1/M2/M3/M4
- Power-constrained deployments
- Real-time applications

### INT8 Quantization

**Best for:** Memory-constrained deployments

```rust
use adapteros_lora_kernel_mtl::{
    QuantizationType, CalibrationConfig, CalibrationMethod,
};

let config = ConversionConfig {
    quantization: Some(QuantizationType::Int8),
    ..Default::default()
};

// Calibration (performed by Python script)
let calibration = CalibrationConfig {
    num_samples: 512,
    method: CalibrationMethod::MinMax,
    accuracy_threshold: 0.99,
    max_steps: 1000,
};
```

**Benefits:**
- ✅ ANE compatible
- ✅ 4x memory reduction
- ✅ Good accuracy with calibration (>99%)

**Trade-offs:**
- ⚠️ Requires calibration data
- ⚠️ Slight accuracy drop (0.5-1%)

### INT4 Quantization (Experimental)

**Best for:** Extreme compression (research)

```rust
let config = ConversionConfig {
    quantization: Some(QuantizationType::Int4),
    ..Default::default()
};
```

**Benefits:**
- ✅ 8x memory reduction
- ✅ Smallest model size

**Trade-offs:**
- ❌ May not be ANE-compatible
- ❌ Larger accuracy drop (2-5%)
- ❌ GPU fallback likely

---

## Weight Mapping

### Supported Architectures

```rust
use adapteros_lora_kernel_mtl::{ArchitectureType, WeightMapper};

// Auto-detect architecture
let arch = ArchitectureType::detect_from_keys(&tensor_names)?;

// Or specify manually
let mapper = WeightMapper::new(ArchitectureType::Qwen25);
```

| Architecture | Support | Notes |
|--------------|---------|-------|
| Qwen2.5 | ✅ Full | Default, tested on Qwen2.5-7B/14B |
| LLaMA 2/3 | ✅ Full | Same structure as Qwen2.5 |
| Mistral | ✅ Full | Same structure as Qwen2.5 |
| GPT-NeoX | ✅ Full | Fused QKV projection |

### Layer Components

```rust
use adapteros_lora_kernel_mtl::{LayerComponent, WeightMapper};

let mapper = WeightMapper::new(ArchitectureType::Qwen25);

// Attention layers
let q_proj = mapper.get_layer_name(LayerComponent::AttentionQProj, 0);
// "model.layers.0.self_attn.q_proj.weight"

let k_proj = mapper.get_layer_name(LayerComponent::AttentionKProj, 0);
// "model.layers.0.self_attn.k_proj.weight"

// Feed-forward layers
let gate_proj = mapper.get_layer_name(LayerComponent::FFNGateProj, 0);
// "model.layers.0.mlp.gate_proj.weight"

// CoreML-friendly names
let coreml_name = mapper.to_coreml_name(LayerComponent::AttentionQProj, 0);
// "layer0_attn_q"
```

### LoRA Adapter Mapping

```rust
use adapteros_lora_kernel_mtl::{LoRAMapping, LoRAComponent};

let lora_mapper = LoRAMapping::new(ArchitectureType::Qwen25);

let result = lora_mapper.map_lora_name(
    "model.layers.0.self_attn.q_proj.lora_A"
)?;

// Returns: ("layer0_attn_q", LoRAComponent::A)
```

---

## Validation

### Numerical Accuracy

```rust
let report = validator.validate_model(original, converted)?;

if let Some(accuracy) = &report.accuracy {
    println!("Mean absolute error: {:.6}", accuracy.mean_absolute_error);
    println!("Mean relative error: {:.6}", accuracy.mean_relative_error);
    println!("Accuracy percentage: {:.2}%", accuracy.accuracy_percentage);

    if accuracy.meets_threshold(1e-3) {
        println!("✅ Accuracy within threshold");
    }
}
```

### ANE Compatibility

```rust
if let Some(ane) = &report.ane_compatibility {
    println!("Fully compatible: {}", ane.fully_compatible);
    println!("Compatibility: {:.1}%", ane.compatibility_percentage);

    if !ane.incompatible_ops.is_empty() {
        println!("⚠️  GPU fallback ops:");
        for op in &ane.incompatible_ops {
            println!("  - {}", op);
        }
    }

    if ane.is_production_ready() {
        println!("✅ Production-ready for ANE");
    }
}
```

### Performance Benchmarks

```rust
if let Some(perf) = &report.performance {
    println!("Throughput: {:.1} tokens/sec", perf.throughput_tokens_per_sec);
    println!("Latency: {:.2} ms", perf.avg_latency_ms);
    println!("Memory: {:.1} MB", perf.peak_memory_mb);
    println!("ANE used: {}", perf.ane_used);

    if perf.meets_target(50.0) {
        println!("✅ Meets target throughput");
    }
}
```

---

## Performance Optimization

### ANE Optimization Checklist

- [x] **Batch size = 1**: ANE optimized for single-sequence inference
- [x] **FP16 quantization**: Native ANE precision
- [x] **Sequence length alignment**: Multiples of 8 for ANE
- [x] **Layer normalization**: Epsilon ≥ 1e-5
- [x] **Avoid custom ops**: Use built-in operations only

### Expected Performance

| Model | Size | Quantization | ANE | Throughput | Latency |
|-------|------|--------------|-----|------------|---------|
| Qwen2.5-7B | 7B params | FP16 | M4 | 45-50 tok/s | 20-22 ms |
| Qwen2.5-7B | 7B params | INT8 | M4 | 50-55 tok/s | 18-20 ms |
| Qwen2.5-14B | 14B params | FP16 | M4 | 25-30 tok/s | 33-40 ms |

---

## Troubleshooting

### Issue: Conversion Script Fails

**Symptom:**
```
ImportError: No module named 'coremltools'
```

**Solution:**
```bash
pip install coremltools torch safetensors transformers
```

### Issue: ANE Not Available

**Symptom:**
```
Validation report: ane_used = false
```

**Causes:**
- Non-Apple Silicon device
- macOS < 13.0
- Model ops not ANE-compatible

**Solution:**
```rust
// Check ANE availability
if !is_neural_engine_available() {
    warn!("ANE not available, will use GPU fallback");
}
```

### Issue: Accuracy Below Threshold

**Symptom:**
```
Mean relative error: 0.005 > 0.001
```

**Causes:**
- INT8/INT4 quantization without calibration
- Numerical precision issues

**Solution:**
```rust
// Use FP16 instead of INT8
let config = ConversionConfig {
    quantization: Some(QuantizationType::Float16),
    ..Default::default()
};

// Or provide calibration data for INT8
```

### Issue: Low Performance

**Symptom:**
```
Throughput: 10 tokens/sec (expected 50)
```

**Causes:**
- GPU fallback (ANE not used)
- Suboptimal batch size
- Custom operations

**Solution:**
```bash
# Check ANE compatibility
python3 -c "
import coremltools as ct
model = ct.models.MLModel('model.mlpackage')
spec = model.get_spec()
# Inspect for custom ops
"
```

---

## Examples

### Complete Workflow

```rust
use adapteros_lora_kernel_mtl::{
    ConversionConfig, ModelConverter, ModelValidator,
    QuantizationType, ValidationConfig,
};
use std::path::Path;

fn convert_and_validate() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Configure
    let config = ConversionConfig {
        quantization: Some(QuantizationType::Float16),
        target_ane: true,
        ..Default::default()
    };

    // Step 2: Convert
    let converter = ModelConverter::new(config)?;
    let manifest = converter.convert_safetensors_to_coreml(
        Path::new("qwen2.5-7b.safetensors"),
        Path::new("qwen2.5-7b.mlpackage"),
    )?;

    println!("Run: python3 {}", manifest.script_path.display());

    // Step 3: Validate (after running Python script)
    let validator = ModelValidator::new(ValidationConfig::default());
    let report = validator.validate_model(
        Path::new("qwen2.5-7b.safetensors"),
        Path::new("qwen2.5-7b.mlpackage"),
    )?;

    // Step 4: Check results
    if report.passed() {
        println!("✅ Conversion successful!");
        if let Some(perf) = &report.performance {
            println!("Throughput: {:.1} tokens/sec", perf.throughput_tokens_per_sec);
        }
    } else {
        println!("❌ Validation failed");
        for error in &report.errors {
            println!("  - {}", error);
        }
    }

    Ok(())
}
```

---

## References

- [docs/COREML_INTEGRATION.md](../../docs/COREML_INTEGRATION.md) - CoreML backend integration
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](../../docs/ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection
- [Apple CoreML Documentation](https://developer.apple.com/documentation/coreml)
- [coremltools Documentation](https://coremltools.readme.io/)

---

**Signed:** James KC Auchterlonie
**Date:** 2025-01-19
