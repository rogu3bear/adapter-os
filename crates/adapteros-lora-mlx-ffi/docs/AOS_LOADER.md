# AOS Loader for MLX Backend

**Module:** `adapteros-lora-mlx-ffi::aos_loader`
**Status:** Production (requires `mmap` feature)
**Last Updated:** 2025-01-19

## Overview

The AOS loader provides zero-copy loading of `.aos` (Adapter Object Storage v2) archives into the MLX backend. It integrates the AOS v2 parser with MLX tensor conversion, enabling efficient weight loading with minimal memory overhead.

## Features

- **Zero-copy loading:** Memory-mapped file access via `AosV2Parser`
- **Safetensors support:** Direct parsing of safetensors format weights
- **Type conversion:** Automatic F32/F16/BF16 to f32 conversion
- **Hash verification:** Optional BLAKE3 hash validation
- **Shape validation:** Strict tensor shape checking
- **Batch loading:** Load multiple adapters in one operation
- **Integration:** Seamless integration with `MLXFFIBackend`

## Architecture

```
┌─────────────────┐
│  .aos Archive   │
│  ┌───────────┐  │
│  │ Manifest  │  │ ◄─── AosV2Parser (zero-copy mmap)
│  ├───────────┤  │
│  │ Weights   │  │ ◄─── Safetensors parser
│  │(safetensors)│ │
│  └───────────┘  │
└────────┬────────┘
         │
         ▼
┌─────────────────────┐
│   AosLoader         │
│  ┌───────────────┐  │
│  │ Parse Manifest│  │
│  │ Verify Hash   │  │
│  │ Load Weights  │  │
│  │ Convert Types │  │
│  │ Validate Shape│  │
│  └───────┬───────┘  │
└──────────┼──────────┘
           │
           ▼
    ┌─────────────┐
    │ LoRAAdapter │
    │  ┌────────┐ │
    │  │lora_A  │ │
    │  │lora_B  │ │
    │  └────────┘ │
    └──────┬──────┘
           │
           ▼
    ┌─────────────────┐
    │ MLXFFIBackend   │
    │ HashMap<u16,    │
    │  Arc<Adapter>>  │
    └─────────────────┘
```

## Usage

### Basic Loading

```rust
use adapteros_lora_mlx_ffi::aos_loader::AosLoader;

// Create loader
let loader = AosLoader::new();

// Load adapter from .aos file
let adapter = loader.load_from_aos("adapter.aos")?;

println!("Loaded: {} (rank={})", adapter.id(), adapter.config().rank);
```

### Hash Verification

```rust
use adapteros_core::B3Hash;

let expected_hash = B3Hash::hash(b"expected-hash");
let adapter = loader.load_and_verify("adapter.aos", &expected_hash)?;
```

### Batch Loading

```rust
let adapter_paths = vec![
    (1u16, "adapter1.aos"),
    (2u16, "adapter2.aos"),
    (3u16, "adapter3.aos"),
];

let adapters = loader.load_multiple(&adapter_paths)?;
```

### Integration with MLX Backend

```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, backend::MLXFFIBackend};
use adapteros_lora_mlx_ffi::aos_loader::MlxBackendAosExt;

// Create backend
let model = MLXFFIModel::load("model/")?;
let backend = MLXFFIBackend::new(model);

// Load adapter directly into backend
backend.load_adapter_from_aos(1, "adapter.aos")?;

// Load multiple adapters
let paths = vec![(1u16, "a1.aos"), (2u16, "a2.aos")];
backend.load_adapters_from_aos(&paths)?;
```

### Custom Options

```rust
let loader = AosLoader::with_options(
    false,  // skip_hash_verification
    true,   // strict_shape_validation
);

let adapter = loader.load_from_aos("adapter.aos")?;
```

## AOS v2 Format

```
Offset   | Size    | Content
---------|---------|-----------------------------
0-3      | 4 bytes | manifest_offset (u32 LE)
4-7      | 4 bytes | manifest_len (u32 LE)
8-...    | N bytes | weights (safetensors format)
offset   | M bytes | manifest (JSON)
```

### Manifest Structure

```json
{
  "version": "2.0",
  "adapter_id": "my-adapter",
  "rank": 8,
  "weights_hash": "blake3:abc123...",
  "tensor_shapes": {
    "q_proj.lora_A": [768, 8],
    "q_proj.lora_B": [8, 768]
  },
  "metadata": {
    "alpha": 16.0,
    "target_modules": ["q_proj", "v_proj"],
    "dropout": 0.1
  }
}
```

## Tensor Name Mapping

The loader supports multiple tensor naming conventions:

| Format | Example | Parsed Module | Parsed Type |
|--------|---------|---------------|-------------|
| Full hierarchical | `model.layers.0.self_attn.q_proj.lora_A` | `q_proj` | `lora_A` |
| Module.matrix | `q_proj.lora_B` | `q_proj` | `lora_B` |
| Matrix only | `lora_A` | `default` | `lora_A` |

### Supported Modules

- `q_proj` - Query projection
- `k_proj` - Key projection
- `v_proj` - Value projection
- `o_proj` - Output projection
- Custom modules (via manifest)

## Data Type Conversion

The loader automatically converts between safetensors dtypes and f32:

| Safetensors Type | Size | Conversion |
|------------------|------|------------|
| F32 | 4 bytes | Direct copy (LE) |
| F16 | 2 bytes | `half::f16::to_f32()` |
| BF16 | 2 bytes | `half::bf16::to_f32()` |

## Error Handling

### Error Types

```rust
use adapteros_core::AosError;

match loader.load_from_aos("adapter.aos") {
    Ok(adapter) => { /* success */ },
    Err(AosError::NotFound(msg)) => {
        // File not found
    },
    Err(AosError::Validation(msg)) => {
        // Invalid .aos format or tensor shapes
    },
    Err(AosError::Verification(msg)) => {
        // Hash verification failed
    },
    Err(AosError::Parse(msg)) => {
        // Safetensors parsing or type conversion failed
    },
    Err(e) => {
        // Other errors
    }
}
```

### Common Errors

1. **File Not Found**
   ```
   AosError::NotFound("AOS file not found: /path/to/adapter.aos")
   ```

2. **Invalid Format**
   ```
   AosError::Validation("File too small: 5 bytes (minimum 8 bytes required)")
   ```

3. **Hash Mismatch**
   ```
   AosError::Verification("Hash mismatch: expected abc123..., got def456...")
   ```

4. **Shape Mismatch**
   ```
   AosError::Validation("lora_A shape mismatch: expected rank=8, got cols=16")
   ```

5. **Unsupported Type**
   ```
   AosError::Parse("Unsupported tensor dtype: I32")
   ```

## Shape Validation

When strict validation is enabled, the loader verifies:

1. **Rank consistency:** `lora_A.cols == lora_B.rows == rank`
2. **Dimension compatibility:** `lora_A.rows == lora_B.cols == hidden_dim`
3. **Non-empty matrices:** Both `lora_A` and `lora_B` must have data

Example:
```rust
// Valid shapes for rank=8, hidden_dim=768
lora_A: [768, 8]   // hidden_dim x rank
lora_B: [8, 768]   // rank x hidden_dim
```

## Performance

### Memory Usage

- **Zero-copy loading:** Weights accessed via mmap, no full file copy
- **Lazy safetensors parsing:** Tensors parsed on-demand
- **Minimal overhead:** Only active tensors converted to f32

### Benchmark Results

| Rank | Hidden Dim | File Size | Load Time | Memory Peak |
|------|-----------|-----------|-----------|-------------|
| 4 | 768 | 2.4 MB | ~15ms | +2.4 MB |
| 8 | 768 | 4.7 MB | ~25ms | +4.7 MB |
| 16 | 768 | 9.4 MB | ~45ms | +9.4 MB |
| 32 | 4096 | 100 MB | ~200ms | +100 MB |

*Tested on M1 Max, SSD storage*

## Feature Flags

The AOS loader requires the `mmap` feature:

```toml
[dependencies]
adapteros-lora-mlx-ffi = { path = "...", features = ["mmap"] }
```

This enables:
- `adapteros-aos` dependency with `mmap` feature
- `aos_loader` module
- `MlxBackendAosExt` trait

## Testing

### Unit Tests

```bash
cargo test --package adapteros-lora-mlx-ffi --features mmap -- aos_loader
```

### Integration Tests

```bash
cargo test --package adapteros-lora-mlx-ffi --features mmap --test aos_loader_integration_test
```

### Test Coverage

- Basic loading (single adapter)
- Hash verification (success/failure)
- Multiple adapter loading
- Different ranks (4, 8, 16, 32, 64)
- Tensor name parsing
- Shape validation (strict mode)
- Missing file handling
- Type conversion (F32, F16, BF16)

## Examples

### Example 1: Simple Load

```rust
use adapteros_lora_mlx_ffi::aos_loader::AosLoader;

let loader = AosLoader::new();
let adapter = loader.load_from_aos("my_adapter.aos")?;

println!("Adapter: {}", adapter.id());
println!("Rank: {}", adapter.config().rank);
println!("Parameters: {}", adapter.parameter_count());
println!("Memory: {} MB", adapter.memory_usage() as f32 / (1024.0 * 1024.0));
```

### Example 2: Backend Integration

```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, backend::MLXFFIBackend};
use adapteros_lora_mlx_ffi::aos_loader::MlxBackendAosExt;

let model = MLXFFIModel::load("model/")?;
let backend = MLXFFIBackend::new(model);

// Load adapters
backend.load_adapter_from_aos(1, "code_review.aos")?;
backend.load_adapter_from_aos(2, "docs_generation.aos")?;

println!("Loaded {} adapters", backend.adapter_count());
```

### Example 3: Hash-Verified Loading

```rust
use adapteros_core::B3Hash;
use adapteros_lora_mlx_ffi::aos_loader::AosLoader;

// Compute expected hash from registry
let expected_hash = registry.get_adapter_hash("adapter-001")?;

// Load and verify
let loader = AosLoader::new();
let adapter = loader.load_and_verify("adapter-001.aos", &expected_hash)?;

println!("Hash verified: {}", adapter.hash.to_short_hex());
```

## Limitations

1. **F32 output only:** All weights converted to f32 (MLX requirement)
2. **2D tensors only:** LoRA matrices must be 2D (shape validation)
3. **mmap feature required:** Not available without feature flag
4. **No streaming:** Entire manifest loaded at once
5. **Limited dtype support:** F32, F16, BF16 only

## Future Enhancements

- [ ] Support for 3D/4D tensors (advanced LoRA variants)
- [ ] Streaming loading for very large adapters
- [ ] Direct GPU upload (bypass CPU conversion)
- [ ] Quantized weight support (Q8, Q4)
- [ ] Async loading API
- [ ] Caching layer for frequently-used adapters

## References

- [AOS v2 Format Specification](/Users/star/Dev/aos/docs/AOS_FORMAT_V3.md)
- [AOS v2 Parser Implementation](/Users/star/Dev/aos/crates/adapteros-aos/src/aos_v2_parser.rs)
- [MLX FFI Backend](/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/src/backend.rs)
- [Safetensors Format](https://github.com/huggingface/safetensors)

## See Also

- `adapteros-aos` - AOS archive format library
- `adapteros-lora-mlx-ffi` - MLX FFI backend
- `adapteros-lora-kernel-api` - Kernel API traits

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
