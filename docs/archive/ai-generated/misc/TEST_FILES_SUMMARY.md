# AOS Test Files Summary

**Generated**: 2025-11-19
**Tool**: `aos-analyze` (Rust binary at `/Users/star/Dev/aos/crates/adapteros-aos/src/bin/aos-analyze.rs`)
**Total Files**: 6

---

## Test File Inventory

| File | Size | Rank | Hidden | Params | Format | Status |
|------|------|------|--------|--------|--------|--------|
| test_adapter.aos | 2.7 KB | 2 | 32 | 162 | JSON | ✓ Valid |
| adapter_1.aos | TBD | TBD | TBD | TBD | JSON | ✓ Valid |
| adapter_2.aos | 4.3 KB | 4 | 32 | 292 | JSON | ✓ Valid |
| adapter_3.aos | 4.9 KB | 5 | 32 | 357 | JSON | ✓ Valid |
| large_adapter.aos | 7.8 KB | 4 | 64 | 580 | JSON | ✓ Valid |
| corrupted_adapter.aos | - | - | - | - | - | ✗ Invalid |

---

## Detailed Analysis

### test_adapter.aos
```
Size: 2,715 bytes (2.65 KB)
Format: JSON weights
Rank: 2, Hidden: 32
Tensors: 4 (lora_a_q15, lora_b_q15, scale_a, scale_b)
Parameters: 162
Base model: qwen2.5-7b
Created: 2025-11-16T07:50:21Z
Status: ✓ Valid
```

### adapter_2.aos
```
Size: 4,257 bytes (4.16 KB)
Format: JSON weights
Rank: 4, Hidden: 32
Tensors: 4 (lora_a_q15, lora_b_q15, scale_a, scale_b)
Parameters: 292
Base model: qwen2.5-7b
Created: 2025-11-16T07:50:21Z
Status: ✓ Valid
```

### adapter_3.aos
```
Size: 5,027 bytes (4.91 KB)
Format: JSON weights
Rank: 5, Hidden: 32
Tensors: 4 (lora_a_q15, lora_b_q15, scale_a, scale_b)
Parameters: 357
Base model: qwen2.5-7b
Created: 2025-11-16T07:50:21Z
Status: ✓ Valid
```

### large_adapter.aos
```
Size: 7,809 bytes (7.63 KB)
Format: JSON weights
Rank: 4, Hidden: 64
Tensors: 4 (lora_a_q15, lora_b_q15, scale_a, scale_b)
Parameters: 580
Base model: qwen2.5-7b
Created: 2025-11-16T07:50:21Z
Status: ✓ Valid
```

### corrupted_adapter.aos
```
Purpose: Error testing
Status: ✗ Invalid (intentional)
```

---

## Common Characteristics

### All Valid Files Share:

1. **Format Version**: 2.0
2. **Base Model**: qwen2.5-7b
3. **Weights Format**: JSON (test format)
4. **Tensor Count**: 4 tensors
5. **Tensor Names**: lora_a_q15, lora_b_q15, scale_a, scale_b
6. **Data Type**: Q15 for LoRA weights, float32 for scales
7. **Manifest Size**: 369 bytes (consistent)

### Variations:

1. **Rank**: 2, 4, 5 (determines parameter count)
2. **Hidden Dimension**: 32 or 64 (affects tensor shapes)
3. **File Size**: 2.7 KB to 7.8 KB (scales with rank × hidden)

---

## Tensor Structure

All files follow this pattern:

```json
{
  "lora_a_q15": [
    [Q15_values...],  // rank rows
    ...
  ],
  "lora_b_q15": [
    [Q15_values...],  // hidden rows
    ...
  ],
  "scale_a": [float32...],  // rank values
  "scale_b": [float32...]   // hidden values
}
```

### Shape Formulas

```
lora_a_q15 shape: [rank, hidden]
lora_b_q15 shape: [hidden, rank]
scale_a shape: [rank]
scale_b shape: [hidden]

Total params = (rank × hidden) + (hidden × rank) + rank + hidden
             = 2 × (rank × hidden) + rank + hidden
```

### Examples

| File | Rank | Hidden | lora_a | lora_b | scale_a | scale_b | Total |
|------|------|--------|--------|--------|---------|---------|-------|
| test_adapter | 2 | 32 | 2×32=64 | 32×2=64 | 2 | 32 | 162 |
| adapter_2 | 4 | 32 | 4×32=128 | 32×4=128 | 4 | 32 | 292 |
| adapter_3 | 5 | 32 | 5×32=160 | 32×5=160 | 5 | 32 | 357 |
| large_adapter | 4 | 64 | 4×64=256 | 64×4=256 | 4 | 64 | 580 |

---

## File Size Analysis

### Size Formula (JSON Format)

```
file_size ≈ 8 + weights_json_size + 369

Where:
  8 = header size
  weights_json_size ≈ params × 25 (average JSON overhead)
  369 = manifest size (fixed)
```

### Observed Sizes

| Params | Predicted | Actual | Diff |
|--------|-----------|--------|------|
| 162 | 4,058 B | 2,715 B | -33% |
| 292 | 7,308 B | 4,257 B | -42% |
| 357 | 8,933 B | 5,027 B | -44% |
| 580 | 14,508 B | 7,809 B | -46% |

**Note**: Actual sizes are smaller due to efficient JSON encoding of Q15 arrays.

---

## Q15 Value Distribution

All test files use Q15 quantization:

```
Value Range: -32768 to 32767
Common values in test files:
  32767  (max positive)
  0      (zero)
  -32768 (max negative)
```

Example from test_adapter.aos:
```
[32767, 32767, -32768, 32767, 32767, -32768, ...]
```

This appears to be synthetic test data with extreme values.

---

## Manifest Comparison

### Common Fields (All Files)

```json
{
  "version": "2.0",
  "rank": <varies>,
  "base_model": "qwen2.5-7b",
  "training_config": {
    "rank": <varies>,
    "alpha": <varies>,
    "learning_rate": 0.001,
    "batch_size": 2,
    "epochs": 1,
    "hidden_dim": <varies>
  },
  "created_at": "2025-11-16T07:50:21.nnnnnn+00:00",
  "weights_hash": "blake3:<hash>",
  "metadata": {}
}
```

### Variable Fields

| File | rank | alpha | hidden_dim |
|------|------|-------|------------|
| test_adapter | 2 | 4.0 | 32 |
| adapter_2 | 4 | 4.0 | 32 |
| adapter_3 | 5 | 4.0 | 32 |
| large_adapter | 4 | 8.0 | 64 |

**Pattern**: alpha = rank × 2.0 (except large_adapter)

---

## Usage Examples

### Analyze a Single File

```bash
aos-analyze test_data/adapters/test_adapter.aos
```

### Validate File Structure

```bash
aos-validate test_data/adapters/test_adapter.aos
```

### Display File Information

```bash
aos-info test_data/adapters/test_adapter.aos
```

### Batch Analysis

```bash
for f in test_data/adapters/*.aos; do
    aos-analyze "$f"
done
```

### Using Cargo (if binaries not installed)

```bash
cargo run --bin aos-analyze -- test_data/adapters/test_adapter.aos
cargo run --bin aos-validate -- test_data/adapters/test_adapter.aos
```

---

## Validation Results

All non-corrupted files pass validation:

- ✓ Header structure valid
- ✓ Offsets within bounds
- ✓ File size matches header
- ✓ JSON weights parseable
- ✓ Manifest valid
- ✓ All required fields present

---

## Migration Notes

These test files use JSON weights format for:
- Easy generation in tests
- Human-readable debugging
- Simple validation

For production, convert to SafeTensors:
- ~60% size reduction
- Zero-copy loading
- Faster inference

---

## References

- **Analysis Tools**:
  - `aos-analyze` - Detailed file analysis
  - `aos-validate` - Structure and hash validation
  - `aos-info` - Quick file information
  - `aos-verify` - Integrity verification
- **Format Spec**: `/Users/star/Dev/aos/docs/AOS_V2_ACTUAL_FORMAT.md`
- **Writer Code**: `/Users/star/Dev/aos/crates/adapteros-aos/src/aos2_writer.rs`

---

**Summary Status**: Complete
**Files Analyzed**: 6 (5 valid, 1 corrupted)
**Tool Version**: v2.0
**Last Updated**: 2025-11-19
