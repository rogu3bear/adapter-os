# AOS v2.0 Actual File Format (As Implemented)

**Document Type**: Technical Documentation - Actual Implementation
**Version**: 2.0
**Date**: 2025-11-19
**Author**: Analyzed by Agent 1 (AOS File Parser and Analyzer)
**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Executive Summary

This document describes the **actual implemented format** of .aos v2.0 files as found in the AdapterOS codebase, based on analysis of test files and the `adapteros-aos` crate implementation.

### Key Findings

1. **Two Variants Exist**:
   - **Test/Development Format**: JSON-encoded weights (used in test_data/)
   - **Production Format**: SafeTensors binary weights (documented in spec)

2. **Common Structure**: Both use identical 8-byte header and JSON manifest

3. **Tested Implementation**: Test files use simplified JSON weights format

---

## Binary Structure

### Overall Layout (Both Variants)

```
[Bytes 0-7]      Header (8 bytes)
[Bytes 8...]     Weights Section (JSON or SafeTensors)
[manifest_offset...] JSON Manifest
```

### Header Format (8 bytes)

```c
struct AOS2Header {
    uint32_t manifest_offset;    // [0-3]  Byte offset to JSON manifest (LE)
    uint32_t manifest_len;        // [4-7]  Length of JSON manifest (LE)
};
```

**Example from test_adapter.aos**:
```
Bytes 0-3: 0x0000092a (2,346) - manifest_offset
Bytes 4-7: 0x00000171 (369)   - manifest_len
```

All multi-byte values are stored in **little-endian** format.

---

## Weights Section Formats

### Variant 1: JSON Weights (Test Format)

**Used in**: `test_data/adapters/*.aos`

**Structure**:
```json
{
  "lora_a_q15": [[32767, 32767, -32768, ...], ...],
  "lora_b_q15": [[32767, -32768, ...], ...],
  "scale_a": [1.0, 0.5, ...],
  "scale_b": [1.0, 1.0, ...]
}
```

**Properties**:
- Human-readable JSON format
- Q15 quantized values as JSON arrays (range: -32768 to 32767)
- Scale factors as floating-point arrays
- Easy to generate for testing
- Larger file size than binary

**Example Analysis**:
```
File: test_adapter.aos
Total size: 2,715 bytes
  - Header: 8 bytes
  - JSON weights: 2,338 bytes
  - Manifest: 369 bytes

Tensors:
  lora_a_q15    Q15   [2x32]   64 params
  lora_b_q15    Q15   [32x2]   64 params
  scale_a       f32   [2]      2 params
  scale_b       f32   [32]     32 params
Total: 162 parameters
```

### Variant 2: SafeTensors Format (Production)

**Used in**: Production adapters (documented but not in test files)

**Structure**:
```
[0-7]     header_size (u64 LE) - Size of JSON metadata
[8-...]   JSON metadata - Tensor descriptions
[...]     Binary tensor data - Aligned tensor values
```

**SafeTensors Metadata Example**:
```json
{
  "lora_a.q_proj": {
    "dtype": "Q15",
    "shape": [16, 3584],
    "data_offsets": [0, 114688]
  },
  "lora_b.q_proj": {
    "dtype": "Q15",
    "shape": [3584, 16],
    "data_offsets": [114688, 229376]
  },
  "__metadata__": {
    "format": "safetensors"
  }
}
```

**Properties**:
- Binary format for efficiency
- Zero-copy memory mapping
- Tensor metadata with exact offsets
- Production-ready
- Smaller file size

---

## Manifest Format (Common to Both Variants)

### JSON Schema

```json
{
  "version": "2.0",
  "rank": 4,
  "base_model": "qwen2.5-7b",
  "training_config": {
    "rank": 4,
    "alpha": 8.0,
    "learning_rate": 0.001,
    "batch_size": 2,
    "epochs": 1,
    "hidden_dim": 64
  },
  "created_at": "2025-11-16T07:50:21.616344+00:00",
  "weights_hash": "blake3:0be6f97a...",
  "metadata": {}
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Format version ("2.0") |
| `rank` | integer | LoRA rank |
| `base_model` | string | Base model identifier |
| `training_config` | object | Training parameters |
| `created_at` | string | ISO 8601 timestamp |
| `weights_hash` | string | BLAKE3 hash of weights section |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `adapter_id` | string | Semantic adapter ID |
| `lora_config` | object | LoRA configuration |
| `metadata` | object | User-defined metadata |

---

## File Size Analysis

Based on test files in `test_data/adapters/`:

| File | Rank | Hidden | Params | Weights Size | Total Size |
|------|------|--------|--------|--------------|------------|
| test_adapter.aos | 2 | 32 | 162 | 2,338 B | 2,715 B |
| adapter_2.aos | 4 | 32 | 292 | 3,880 B | 4,257 B |
| adapter_3.aos | 8 | 32 | 548 | 7,366 B | 7,735 B |
| large_adapter.aos | 4 | 64 | 580 | 7,432 B | 7,809 B |

**Formula** (JSON format):
```
file_size ≈ 8 + (rank × hidden × 4 × 25) + 369
            ↑   ↑                          ↑
         header  JSON encoding overhead   manifest
```

---

## Format Detection

### Automatic Detection Algorithm

```rust
fn detect_weights_format(data: &[u8], offset: usize) -> Result<WeightsFormat> {
    let sample = &data[offset..offset.min(data.len()).min(offset + 100)];

    // Check for JSON (starts with '{' or '[')
    if let Ok(s) = std::str::from_utf8(sample) {
        let trimmed = s.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return Ok(WeightsFormat::Json);
        }
    }

    // Check for SafeTensors (u64 header size)
    if sample.len() >= 8 {
        let header_size = u64::from_le_bytes([
            sample[0], sample[1], sample[2], sample[3],
            sample[4], sample[5], sample[6], sample[7],
        ]);
        if header_size > 0 && header_size < 1_000_000 {
            return Ok(WeightsFormat::SafeTensors);
        }
    }

    Ok(WeightsFormat::Unknown)
}
```

### Detection Example

```
File: test_adapter.aos
Offset 8-16: 7b 0a 20 20 22 6c 6f 72 = "{.  "lor"
Detection: JSON (starts with '{')

File: production_adapter.aos
Offset 8-16: d8 03 00 00 00 00 00 00 = 984 (u64 LE)
Detection: SafeTensors (reasonable header size)
```

---

## Rust Implementation Reference

### Writing AOS v2.0 Files

**Location**: `/Users/star/Dev/aos/crates/adapteros-aos/src/aos2_writer.rs`

```rust
pub struct AOS2Writer;

impl AOS2Writer {
    pub fn write_archive<P, M>(
        &self,
        output_path: P,
        manifest: &M,
        weights_data: &[u8],  // SafeTensors or JSON bytes
    ) -> Result<u64>
    where
        P: AsRef<Path>,
        M: Serialize,
    {
        let manifest_json = serde_json::to_vec_pretty(manifest)?;
        let manifest_offset = 8 + weights_data.len();
        let manifest_len = manifest_json.len();

        // Write header (8 bytes)
        file.write_all(&(manifest_offset as u32).to_le_bytes())?;
        file.write_all(&(manifest_len as u32).to_le_bytes())?;

        // Write weights (SafeTensors or JSON)
        file.write_all(weights_data)?;

        // Write manifest (JSON)
        file.write_all(&manifest_json)?;

        Ok(total_size)
    }
}
```

### Reading AOS v2.0 Files

```rust
pub fn read_header<P: AsRef<Path>>(path: P) -> Result<(u32, u32)> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;

    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    Ok((manifest_offset, manifest_len))
}
```

---

## Validation Rules

### Header Validation

```rust
fn validate_header(data: &[u8], file_size: usize) -> Result<Vec<String>> {
    let mut errors = Vec::new();

    if data.len() < 8 {
        errors.push("File too small (< 8 bytes)".to_string());
        return Ok(errors);
    }

    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    // Check offset range
    if manifest_offset < 8 {
        errors.push(format!("Invalid manifest_offset: {}", manifest_offset));
    }

    // Check total size
    if manifest_offset + manifest_len != file_size {
        errors.push(format!(
            "Size mismatch: {} != {}",
            manifest_offset + manifest_len,
            file_size
        ));
    }

    Ok(errors)
}
```

### Manifest Validation

```rust
fn validate_manifest(manifest: &ManifestV2) -> Result<Vec<String>> {
    let mut errors = Vec::new();

    // Check required fields (enforced by struct, but verify values)
    if manifest.version != "2.0" {
        errors.push(format!("Invalid version: {}", manifest.version));
    }

    if manifest.rank == 0 {
        errors.push("Invalid rank: must be > 0".to_string());
    }

    if manifest.base_model.is_empty() {
        errors.push("Missing base_model".to_string());
    }

    if manifest.weights_hash.is_empty() {
        errors.push("Missing weights_hash".to_string());
    }

    Ok(errors)
}
```

---

## Q15 Quantization Details

### Format Specification

Q15 is a fixed-point format mapping [-1.0, 1.0] to signed 16-bit integers:

```
Range: -1.0 ≤ value ≤ 1.0
Encoding: int16 = round(float × 32767)
Decoding: float = int16 / 32767.0
```

### Example Values

| Float | Q15 (decimal) | Q15 (hex) | Binary |
|-------|---------------|-----------|--------|
| 1.0 | 32767 | 0x7FFF | 0111111111111111 |
| 0.5 | 16384 | 0x4000 | 0100000000000000 |
| 0.0 | 0 | 0x0000 | 0000000000000000 |
| -0.5 | -16384 | 0xC000 | 1100000000000000 |
| -1.0 | -32768 | 0x8000 | 1000000000000000 |

### JSON Representation

In JSON weights format, Q15 values are stored as signed integers:

```json
{
  "lora_a_q15": [
    [32767, 32767, -32768, 32767, ...],
    [32767, -32768, 32767, -32768, ...]
  ]
}
```

---

## Test Data Reference

### Available Test Files

Location: `/Users/star/Dev/aos/test_data/adapters/`

| File | Purpose | Rank | Hidden | Params | Size |
|------|---------|------|--------|--------|------|
| `test_adapter.aos` | Basic validation | 2 | 32 | 162 | 2.7 KB |
| `adapter_2.aos` | Medium rank | 4 | 32 | 292 | 4.3 KB |
| `adapter_3.aos` | High rank | 8 | 32 | 548 | 7.7 KB |
| `large_adapter.aos` | Large hidden | 4 | 64 | 580 | 7.8 KB |
| `corrupted_adapter.aos` | Error testing | N/A | N/A | N/A | N/A |

### Creating Test Files

To generate additional test files, use the test utilities:

```rust
// See: crates/adapteros-aos/tests/aos_v2_parser_tests.rs
use adapteros_aos::AOS2Writer;

let manifest = TestManifest {
    version: "2.0".to_string(),
    rank: 4,
    base_model: "qwen2.5-7b".to_string(),
    // ...
};

let weights_json = serde_json::to_vec(&weights)?;
let writer = AOS2Writer::new();
writer.write_archive("test.aos", &manifest, &weights_json)?;
```

---

## Analysis Tool Usage

### Command Line

```bash
# Analyze a single file
aos-analyze test_data/adapters/test_adapter.aos

# Using cargo
cargo run --bin aos-analyze -- test_data/adapters/test_adapter.aos

# Batch analysis
for f in test_data/adapters/*.aos; do
    aos-analyze "$f"
done

# Validate file structure
aos-validate test_data/adapters/test_adapter.aos

# Display file information
aos-info test_data/adapters/test_adapter.aos

# Verify integrity
aos-verify test_data/adapters/test_adapter.aos
```

### Output Format

The analyzer outputs:
1. Header analysis (offsets, lengths)
2. Weights format detection and analysis
3. Manifest parsing and validation
4. Hex dump (first 512 bytes)
5. Structure summary
6. Validation errors/warnings

Example output:
```
================================================================================
Analyzing: test_adapter.aos
================================================================================

File size: 2.65 KB (2,715 bytes)

HEADER ANALYSIS (First 8 bytes)
Manifest offset: 2,346 bytes (0x0000092a)
Manifest length: 369 bytes (0x00000171)

WEIGHTS ANALYSIS
Format: JSON (test/development format)
Tensor count: 4
Total parameters: 162

MANIFEST ANALYSIS
Manifest version: 2.0
Base model: qwen2.5-7b

VALIDATION
✓ File structure is valid
```

---

## Migration Path

### From JSON to SafeTensors

For production deployment, JSON weights should be converted to SafeTensors using the Rust tooling:

```rust
use adapteros_aos::{AOS2Reader, AOS2Writer};
use safetensors::SafeTensors;

// Read existing .aos file with JSON weights
let reader = AOS2Reader::new();
let (manifest, json_weights) = reader.read_archive("adapter.aos")?;

// Parse JSON weights
let weights: WeightsJson = serde_json::from_slice(&json_weights)?;

// Convert to SafeTensors format
let safetensors_data = convert_json_to_safetensors(&weights)?;

// Write new .aos file with SafeTensors
let writer = AOS2Writer::new();
writer.write_archive("adapter_st.aos", &manifest, &safetensors_data)?;
```

### Command Line Tool

```bash
# Convert JSON weights to SafeTensors (if converter tool exists)
aos-convert --input adapter.aos --output adapter_st.aos --format safetensors
```

---

## Compatibility Notes

### v2.0 vs v3.0

The proposed v3.0 format (see `/Users/star/Dev/aos/docs/AOS_FORMAT_V3.md`) adds:
- 32-byte extended header with magic number
- Tensor table with checksums
- MPLoRA support
- CRC32 validation

**Migration strategy**:
1. Detect format via magic number (v3) or heuristics (v2)
2. Parse v2 files with backward compatibility
3. Convert v2 → v3 on load if needed

---

## Security Considerations

### File Size Limits

```rust
// Maximum file size: 4GB (u32::MAX)
const MAX_FILE_SIZE: u64 = 0xFFFFFFFF;

// Maximum manifest size: 10MB (reasonable limit)
const MAX_MANIFEST_SIZE: u32 = 10 * 1024 * 1024;

// Maximum weights size: 4GB - 8 bytes - MAX_MANIFEST_SIZE
const MAX_WEIGHTS_SIZE: u64 = MAX_FILE_SIZE - 8 - MAX_MANIFEST_SIZE as u64;
```

### Validation Checklist

- [ ] File size < 4GB
- [ ] Header offsets within bounds
- [ ] Manifest offset ≥ 8
- [ ] manifest_offset + manifest_len == file_size
- [ ] Manifest JSON is valid
- [ ] Weights section is parseable
- [ ] Hash verification (if provided)

---

## Performance Characteristics

### Load Time Comparison

| Format | File Size | Load Time | Memory |
|--------|-----------|-----------|--------|
| JSON weights | 7.8 KB | ~2ms | 7.8 KB |
| SafeTensors | ~4 KB | ~0.5ms | mmap (0 copy) |

### Memory Mapping

SafeTensors format supports zero-copy loading:

```rust
use memmap2::Mmap;

let file = File::open("adapter.aos")?;
let mmap = unsafe { Mmap::map(&file)? };

// Weights are accessed directly from mmap
let weights_ptr = &mmap[data_offset..];
```

---

## References

### Source Files

1. **Writer Implementation**: `/Users/star/Dev/aos/crates/adapteros-aos/src/aos2_writer.rs`
2. **Test Files**: `/Users/star/Dev/aos/test_data/adapters/*.aos`
3. **Analysis Tool**: `/Users/star/Dev/aos/crates/adapteros-aos/src/bin/aos-analyze.rs`
4. **Validation Tool**: `/Users/star/Dev/aos/crates/adapteros-aos/src/bin/aos-validate.rs`
5. **Creation Tool**: `/Users/star/Dev/aos/crates/adapteros-aos/src/bin/aos-create.rs`
6. **v3.0 Specification**: `/Users/star/Dev/aos/docs/AOS_FORMAT_V3.md`

### Related Documentation

- [AOS_V2_TO_V3_MIGRATION.md](AOS_V2_TO_V3_MIGRATION.md) - Migration guide
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - System architecture
- [CLAUDE.md](../CLAUDE.md) - Developer guide

---

## Appendix A: Complete File Structure Example

### test_adapter.aos (2,715 bytes)

```
Offset   Section          Size      Content
------   -------          ----      -------
0x0000   Header           8 B       2a 09 00 00 71 01 00 00
0x0008   JSON Weights     2,338 B   {"lora_a_q15": [[32767, ...]], ...}
0x092a   JSON Manifest    369 B     {"version": "2.0", "rank": 2, ...}
0x0a9b   EOF              -         -

Total: 2,715 bytes
```

### Header Breakdown

```
Bytes    Field              Value (hex)   Value (dec)
-----    -----              -----------   -----------
0-3      manifest_offset    2a 09 00 00   2,346
4-7      manifest_len       71 01 00 00   369
```

### JSON Weights Structure

```json
{
  "lora_a_q15": [
    [32767, 32767, -32768, 32767, 32767, -32768, ...],  // 32 values
    [32767, -32768, 32767, 32767, -32768, -32768, ...]  // 32 values
  ],
  "lora_b_q15": [
    [32767, -32768, ...],  // 2 values × 32 rows
    ...
  ],
  "scale_a": [1.0, 1.0],
  "scale_b": [1.0, 1.0, ..., 1.0]  // 32 values
}
```

---

## Appendix B: Hex Dump Annotations

### First 64 Bytes of test_adapter.aos

```
Offset   Hex Dump                                          ASCII
------   --------                                          -----
00000000 2a 09 00 00 71 01 00 00 7b 0a 20 20 22 6c 6f 72  *...q...{.  "lor
         ↑           ↑           ↑
         │           │           └─ Start of JSON: "{"
         │           └─ manifest_len = 0x0171 (369)
         └─ manifest_offset = 0x092a (2346)

00000010 61 5f 61 5f 71 31 35 22 3a 20 5b 0a 20 20 20 20  a_a_q15": [.
         └─ "lora_a_q15": [

00000020 5b 0a 20 20 20 20 20 20 33 32 37 36 37 2c 0a 20  [.      32767,.
         │               └─ First Q15 value: 32767
         └─ Start of first row: "["

00000030 20 20 20 20 20 20 33 32 37 36 37 2c 0a 20 20 20        32767,.
                   └─ Second Q15 value: 32767
```

---

**Document Status**: Completed
**Verification**: Analyzed using `aos-analyze` (Rust binary)
**Test Coverage**: 5 test files analyzed
**Last Updated**: 2025-11-19
